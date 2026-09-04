use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, LazyLock, Mutex},
};

use config::Config;
use error::Error;
use futures::StreamExt;
use niri::{Snapshot, WorkspaceInfo};
use output::Matcher;
use state::{Event, State};
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use waybar_cffi::{
    Module,
    gtk::{
        self, Orientation, gio,
        glib::MainContext,
        prelude::{BoxExt, ContainerExt, StyleContextExt, WidgetExt},
    },
    waybar_module,
};
use workspace_slot::WorkspaceSlot;

mod animate;
mod column;
mod config;
mod error;
mod glyph;
mod icon;
mod niri;
mod notify;
mod output;
mod process;
mod state;
mod tab;
mod workspace_slot;

static TRACING: LazyLock<()> = LazyLock::new(|| {
    if let Err(e) = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::CLOSE)
        .try_init()
    {
        eprintln!("cannot install global tracing subscriber: {e}");
    }
});

struct TaskbarModule {}

impl Module for TaskbarModule {
    type Config = Config;

    fn init(info: &waybar_cffi::InitInfo, config: Config) -> Self {
        // Ensure tracing-subscriber is initialised.
        *TRACING;

        let module = Self {};
        let state = State::new(config);

        let context = MainContext::default();
        if let Err(e) = context.block_on(init(info, state)) {
            tracing::error!(%e, "Colonnade module init failed");
        }

        module
    }
}

waybar_module!(TaskbarModule);

#[tracing::instrument(level = "DEBUG", skip_all, err)]
async fn init(info: &waybar_cffi::InitInfo, state: State) -> Result<(), Error> {
    let root = info.get_root_widget();
    let container = gtk::Box::new(Orientation::Horizontal, 0);
    container.style_context().add_class("colonnade");
    root.add(&container);

    let context = MainContext::default();
    context.spawn_local(async move { Instance::new(state, container).task().await });

    Ok(())
}

/// Which output this bar instance is on, and that output's logical width
/// in pixels -- the latter is what column widths are proportional to (see
/// BEHAVIOR.md's "Layout" section and `column.rs`'s `REFERENCE_WIDTH_PX`
/// note on what "proportional" actually means here).
#[derive(Debug, Clone)]
struct OutputContext {
    filter: output::Filter,
    width: f64,
}

impl OutputContext {
    fn show_all(width: f64) -> Self {
        Self {
            filter: output::Filter::ShowAll,
            width,
        }
    }
}

struct Instance {
    slots: BTreeMap<u64, WorkspaceSlot>,
    container: gtk::Box,
    last_snapshot: Option<Snapshot>,
    state: State,
}

impl Instance {
    pub fn new(state: State, container: gtk::Box) -> Self {
        Self {
            slots: Default::default(),
            container,
            last_snapshot: None,
            state,
        }
    }

    pub async fn task(&mut self) {
        // We have to build the output context here, because until the Glib event loop has run the
        // container hasn't been realised, which means we can't figure out which output we're on.
        let output_context = Arc::new(Mutex::new(self.build_output_context().await));

        let mut stream = match self.state.event_stream() {
            Ok(stream) => Box::pin(stream),
            Err(e) => {
                tracing::error!(%e, "error starting event stream");
                return;
            }
        };
        while let Some(event) = stream.next().await {
            match event {
                // Notification-driven "urgent" highlighting isn't wired
                // into the fused layout yet -- a Phase 1 scope cut, not
                // dropped for good. The notify stream keeps running
                // regardless (harmless); this just doesn't act on it yet.
                Event::Notification(_) => {}
                Event::WindowSnapshot(snapshot) => {
                    self.process_window_snapshot(snapshot, output_context.clone())
                        .await
                }
                Event::Workspaces(_) => {
                    // We're just using this as a signal that the outputs may have changed.
                    let new_context = self.build_output_context().await;
                    *output_context.lock().expect("output context lock") = new_context;
                }
            }
        }
    }

    #[tracing::instrument(level = "DEBUG", skip(self))]
    async fn build_output_context(&self) -> OutputContext {
        // See upstream's original build_output_filter for the full story on why matching a Gdk 3
        // monitor to a Niri output is this convoluted -- Gdk 3 doesn't expose the Wayland output
        // name, so we fall back to matching geometry/make/model and hope for the best.
        let niri = *self.state.niri();
        let outputs = match gio::spawn_blocking(move || niri.outputs()).await {
            Ok(Ok(outputs)) => outputs,
            Ok(Err(e)) => {
                tracing::warn!(%e, "cannot get Niri outputs");
                return OutputContext::show_all(DEFAULT_WIDTH_PX);
            }
            Err(_) => {
                tracing::error!("error received from gio while waiting for task");
                return OutputContext::show_all(DEFAULT_WIDTH_PX);
            }
        };

        let fallback_width = outputs
            .values()
            .next()
            .and_then(|o| o.logical.as_ref())
            .map(|l| l.width as f64)
            .unwrap_or(DEFAULT_WIDTH_PX);

        if self.state.config().show_all_outputs() || outputs.len() == 1 {
            return OutputContext::show_all(fallback_width);
        }

        let Some(window) = self.container.window() else {
            tracing::warn!("cannot get Gdk window for container");
            return OutputContext::show_all(fallback_width);
        };

        let display = window.display();
        let Some(monitor) = display.monitor_at_window(&window) else {
            tracing::warn!(display = ?window.display(), geometry = ?window.geometry(), "cannot get monitor for window");
            return OutputContext::show_all(fallback_width);
        };

        for (name, output) in outputs.into_iter() {
            let matches = output::Matcher::new(&monitor, &output);
            if matches == Matcher::all() {
                let width = output
                    .logical
                    .as_ref()
                    .map(|l| l.width as f64)
                    .unwrap_or(fallback_width);
                return OutputContext {
                    filter: output::Filter::Only(name),
                    width,
                };
            }
        }

        tracing::warn!(?monitor, "no Niri output matched the Gdk monitor");
        OutputContext::show_all(fallback_width)
    }

    /// Renders one snapshot: fused workspaces + tabs, per BEHAVIOR.md's
    /// "Layout" section. Only the workspace that's `is_active` **on this
    /// bar's own output** blooms into full tabs -- see BEHAVIOR.md's
    /// "Multi-monitor" section for why that's `is_active`, not the single
    /// globally `is_focused` workspace.
    #[tracing::instrument(level = "DEBUG", skip(self, ctx))]
    async fn process_window_snapshot(
        &mut self,
        snapshot: Snapshot,
        ctx: Arc<Mutex<OutputContext>>,
    ) {
        let (filter, output_width) = {
            let ctx = ctx.lock().expect("output context lock");
            (ctx.filter.clone(), ctx.width)
        };

        let workspaces: Vec<&WorkspaceInfo> = snapshot
            .workspaces
            .iter()
            .filter(|ws| filter.should_show(ws.output.as_deref().unwrap_or_default()))
            .collect();

        let bloomed_id = workspaces.iter().find(|ws| ws.is_active).map(|ws| ws.id);

        let mut seen = BTreeSet::new();
        for ws in &workspaces {
            let ws_windows: Vec<_> = snapshot
                .windows
                .iter()
                .filter(|w| w.workspace_id == Some(ws.id))
                .cloned()
                .collect();
            let is_bloomed = Some(ws.id) == bloomed_id;

            // Empty + not bloomed: hidden entirely (BEHAVIOR.md). Empty +
            // bloomed still gets a slot, rendered as an empty tab group --
            // matches "a focused empty workspace shows a single dim `·`"
            // once workspace_slot.rs grows that case; for now it's simply
            // an empty (invisible) group, which is an acceptable Phase 1
            // gap since it's a rare state (an output with literally no
            // windows anywhere).
            if ws_windows.is_empty() && !is_bloomed {
                continue;
            }

            seen.insert(ws.id);
            let slot = self.slots.entry(ws.id).or_insert_with(|| {
                let slot = WorkspaceSlot::new(&self.state, ws.id);
                self.container.add(slot.widget());
                slot
            });

            if is_bloomed {
                slot.set_bloomed(ws, &ws_windows, output_width);
            } else {
                slot.set_collapsed(ws, &snapshot.windows);
            }
        }

        let removed: Vec<u64> = self
            .slots
            .keys()
            .copied()
            .filter(|id| !seen.contains(id))
            .collect();
        for id in removed {
            if let Some(slot) = self.slots.remove(&id) {
                self.container.remove(slot.widget());
            }
        }

        // Keep slot ordering matching workspace idx order (BEHAVIOR.md:
        // stable, never by recency).
        for (i, ws) in workspaces.iter().enumerate() {
            if let Some(slot) = self.slots.get(&ws.id) {
                self.container.reorder_child(slot.widget(), i as i32);
            }
        }

        self.container.show_all();
        self.last_snapshot = Some(snapshot);
    }
}

/// Fallback output width when Niri's own outputs list can't be fetched at
/// all (IPC error) -- an arbitrary but reasonable guess, only used until
/// the next successful outputs() call corrects it.
const DEFAULT_WIDTH_PX: f64 = 1920.0;
