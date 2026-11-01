use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use eframe::egui::{self, Context, ViewportCommand};
use eframe::{App, CreationContext, NativeOptions};

use crate::load::{self, DocKind, Opened};
use crate::sheet::Sheet;
use crate::theme;
use crate::ui;

pub fn run(paths: Vec<PathBuf>) -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 840.0])
            .with_min_inner_size([860.0, 540.0])
            .with_title("Tablo")
            .with_app_id("tablo"),
        ..Default::default()
    };
    eframe::run_native(
        "Tablo",
        options,
        Box::new(move |cc| Ok(Box::new(TabloApp::new(cc, paths)))),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Table,
    Tree,
}

pub struct TabloApp {
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub pending: Vec<PendingLoad>,
    pub error: Option<String>,
    pub help: bool,
    pub toast: Option<Toast>,
    pub focus_filter: bool,
    next_id: u64,
}

pub struct PendingLoad {
    pub name: String,
    rx: Receiver<Result<Opened, String>>,
}

pub struct Toast {
    pub text: String,
    pub until: Option<f64>,
}

pub struct Tab {
    pub id: u64,
    pub opened: Opened,
    pub filter: String,
    pub filtered: Vec<usize>,
    pub sort: Option<(usize, bool)>,
    pub selected: Option<(usize, usize)>,
    pub view: ViewMode,
    pub scroll_to: Option<usize>,
    pub show_inspector: bool,
    pub goto: bool,
    pub goto_buf: String,
    pub tree_pick: Option<(String, String)>,
    last_filter: String,
    last_sort: Option<(usize, bool)>,
    last_header: bool,
}

impl TabloApp {
    fn new(cc: &CreationContext<'_>, paths: Vec<PathBuf>) -> Self {
        theme::install(&cc.egui_ctx);
        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            pending: Vec::new(),
            error: None,
            help: false,
            toast: None,
            focus_filter: false,
            next_id: 1,
        };
        for path in paths {
            app.queue_path(path);
        }
        app
    }

    pub fn current(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn current_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    pub fn queue_path(&mut self, path: PathBuf) {
        self.queue_path_inner(path, false);
    }

    fn queue_path_inner(&mut self, path: PathBuf, reload: bool) {
        if !reload {
            if let Some(i) = self
                .tabs
                .iter()
                .position(|t| t.opened.path == path && path.as_os_str() != "-")
            {
                self.active = i;
                return;
            }
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("fichier")
            .to_string();
        self.pending.push(PendingLoad {
            name,
            rx: spawn_load(path),
        });
    }

    pub fn queue_bytes(&mut self, name: String, bytes: Vec<u8>) {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let path = PathBuf::from(&name);
            let _ = tx.send(load::open_bytes(path, name, &bytes));
        });
        self.pending.push(PendingLoad {
            name: "mémoire".into(),
            rx,
        });
    }

    pub fn open_dialog(&mut self) {
        let files = rfd::FileDialog::new()
            .add_filter("Données", &["csv", "tsv", "json", "ndjson", "jsonl", "txt"])
            .add_filter("CSV", &["csv", "tsv", "txt"])
            .add_filter("JSON", &["json", "ndjson", "jsonl"])
            .pick_files();
        if let Some(files) = files {
            for path in files {
                self.queue_path(path);
            }
        }
    }

    pub fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        if index < self.active {
            self.active -= 1;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
    }

    pub fn reload_current(&mut self) {
        let Some(tab) = self.current() else {
            return;
        };
        let path = tab.opened.path.clone();
        if path.as_os_str() == "-" || !path.exists() {
            self.error = Some("Ce document ne peut pas être rechargé depuis le disque.".into());
            return;
        }
        self.queue_path_inner(path, true);
    }

    pub fn export_current(&mut self) {
        let Some(tab) = self.current() else {
            return;
        };
        let sheet = &tab.opened.sheet;
        let rows = &tab.filtered;
        let stem = tab
            .opened
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("export");
        let default = format!("{stem}-filtre.csv");
        let Some(path) = rfd::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .add_filter("JSON", &["json"])
            .set_file_name(&default)
            .save_file()
        else {
            return;
        };
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("csv")
            .to_ascii_lowercase();
        let result = if ext == "json" {
            sheet
                .to_json(rows)
                .and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string()))
        } else {
            sheet
                .to_csv(rows, tab.opened.kind.delimiter())
                .and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string()))
        };
        match result {
            Ok(()) => self.toast(format!("Exporté vers {}", path.display())),
            Err(e) => self.error = Some(format!("Export impossible : {e}")),
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast {
            text: text.into(),
            until: None,
        });
    }

    fn push_tab(&mut self, opened: Opened) {
        if let Some(i) = self.tabs.iter().position(|t| t.opened.path == opened.path) {
            let old = &self.tabs[i];
            let filter = old.filter.clone();
            let sort = old.sort;
            let view = old.view;
            let show_inspector = old.show_inspector;
            let mut tab = Tab::new(self.next_id, opened);
            tab.filter = filter;
            tab.sort = sort;
            tab.show_inspector = show_inspector;
            if tab.is_json() {
                tab.view = view;
            }
            tab.rebuild();
            self.tabs[i] = tab;
            self.next_id += 1;
            self.active = i;
            return;
        }
        let tab = Tab::new(self.next_id, opened);
        self.next_id += 1;
        self.active = self.tabs.len();
        self.tabs.push(tab);
    }

    fn poll_loads(&mut self) {
        let mut i = 0;
        while i < self.pending.len() {
            match self.pending[i].rx.try_recv() {
                Ok(Ok(opened)) => {
                    self.pending.remove(i);
                    self.push_tab(opened);
                }
                Ok(Err(e)) => {
                    self.pending.remove(i);
                    self.error = Some(e);
                }
                Err(TryRecvError::Empty) => i += 1,
                Err(TryRecvError::Disconnected) => {
                    self.pending.remove(i);
                    self.error = Some("Chargement interrompu.".into());
                }
            }
        }
    }

    fn ingest_drops(&mut self, ctx: &Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            let path = file.path().to_path_buf();
            if path.exists() {
                self.queue_path(path);
                continue;
            }
            match file.bytes() {
                Ok(bytes) => {
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .filter(|s| !s.is_empty())
                        .unwrap_or("glissé")
                        .to_string();
                    self.queue_bytes(name, bytes);
                }
                Err(e) => self.error = Some(format!("Fichier glissé illisible : {e}")),
            }
        }
    }
}

impl Tab {
    pub(crate) fn new(id: u64, opened: Opened) -> Self {
        let view = match opened.kind {
            DocKind::Json { tabular: false } => ViewMode::Tree,
            _ => ViewMode::Table,
        };
        let mut tab = Self {
            id,
            filter: String::new(),
            filtered: Vec::new(),
            sort: None,
            selected: None,
            view,
            scroll_to: None,
            show_inspector: true,
            goto: false,
            goto_buf: String::new(),
            tree_pick: None,
            last_filter: String::from("__init__"),
            last_sort: None,
            last_header: opened.sheet.has_header,
            opened,
        };
        tab.rebuild();
        tab
    }

    pub fn sheet(&self) -> &Sheet {
        &self.opened.sheet
    }

    pub fn sheet_mut(&mut self) -> &mut Sheet {
        &mut self.opened.sheet
    }

    pub fn is_json(&self) -> bool {
        self.opened.json.is_some()
    }

    pub fn rebuild_if_needed(&mut self) {
        if self.filter != self.last_filter
            || self.sort != self.last_sort
            || self.opened.sheet.has_header != self.last_header
        {
            self.rebuild();
        }
    }

    pub fn rebuild(&mut self) {
        let selected_data = if self.opened.sheet.has_header == self.last_header {
            self.selected
                .and_then(|(r, c)| self.filtered.get(r).map(|&data| (data, c)))
        } else {
            None
        };
        let mut idx = self.opened.sheet.filter(&self.filter);
        if let Some((col, asc)) = self.sort {
            self.opened.sheet.sort_indices(&mut idx, col, asc);
        }
        self.selected = selected_data
            .and_then(|(data, c)| idx.iter().position(|&r| r == data).map(|pos| (pos, c)));
        self.scroll_to = self.selected.map(|(r, _)| r);
        self.filtered = idx;
        self.last_filter = self.filter.clone();
        self.last_sort = self.sort;
        self.last_header = self.opened.sheet.has_header;
    }

    pub fn move_sel(&mut self, dr: i32, dc: i32) {
        let rows = self.filtered.len();
        let cols = self.opened.sheet.col_count();
        if rows == 0 || cols == 0 {
            return;
        }
        let Some((r, c)) = self.selected else {
            self.selected = Some((0, 0));
            self.scroll_to = Some(0);
            self.show_inspector = true;
            return;
        };
        let nr = (r as i32 + dr).clamp(0, rows as i32 - 1) as usize;
        let nc = (c as i32 + dc).clamp(0, cols as i32 - 1) as usize;
        self.selected = Some((nr, nc));
        self.scroll_to = Some(nr);
        self.show_inspector = true;
    }

    pub fn selected_text(&self) -> Option<String> {
        let (r, c) = self.selected?;
        let data_row = *self.filtered.get(r)?;
        Some(self.opened.sheet.cell(data_row, c).to_string())
    }

    pub fn selected_data_row(&self) -> Option<usize> {
        let (r, _) = self.selected?;
        self.filtered.get(r).copied()
    }

    pub fn goto_line(&mut self) {
        let Ok(n) = self.goto_buf.trim().parse::<usize>() else {
            return;
        };
        if n == 0 {
            return;
        }
        let target = n - 1;
        if let Some(pos) = self.filtered.iter().position(|&i| i == target) {
            let col = self.selected.map(|(_, c)| c).unwrap_or(0);
            self.selected = Some((pos, col));
            self.scroll_to = Some(pos);
            self.goto = false;
        } else if target < self.opened.sheet.row_count() {
            self.filter.clear();
            self.rebuild();
            if let Some(pos) = self.filtered.iter().position(|&i| i == target) {
                self.selected = Some((pos, 0));
                self.scroll_to = Some(pos);
            }
            self.goto = false;
        }
    }
}

impl App for TabloApp {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_loads();
        self.ingest_drops(ctx);
        if !self.pending.is_empty() {
            ctx.request_repaint();
        }
        if let Some(tab) = self.current() {
            let title = format!("{} — Tablo", tab.opened.name);
            ctx.send_viewport_cmd(ViewportCommand::Title(title));
        } else {
            ctx.send_viewport_cmd(ViewportCommand::Title("Tablo".into()));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui::show(self, ui);
    }
}

fn spawn_load(path: PathBuf) -> Receiver<Result<Opened, String>> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("tablo-load".into())
        .spawn(move || {
            let result = if path.as_os_str() == "-" {
                load_stdin()
            } else {
                load::open_path(&path)
            };
            let _ = tx.send(result);
        })
        .ok();
    rx
}

fn load_stdin() -> Result<Opened, String> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::io::stdin()
        .read_to_end(&mut buf)
        .map_err(|e| format!("stdin : {e}"))?;
    load::open_bytes(PathBuf::from("-"), "(stdin)".into(), &buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: u64) -> Tab {
        let name = format!("{id}.csv");
        let opened = load::open_bytes(
            PathBuf::from(&name),
            name,
            b"nom,score\nAda,30\nGrace,10\nLinus,20\n",
        )
        .unwrap();
        Tab::new(id, opened)
    }

    #[test]
    fn sorting_and_filtering_keep_the_same_selected_data() {
        let mut tab = tab(1);
        tab.selected = Some((0, 0));
        tab.sort = Some((1, true));
        tab.rebuild_if_needed();
        assert_eq!(tab.selected, Some((2, 0)));
        assert_eq!(tab.selected_text().as_deref(), Some("Ada"));
        tab.filter = "ada".into();
        tab.rebuild_if_needed();
        assert_eq!(tab.selected, Some((0, 0)));
        assert_eq!(tab.selected_text().as_deref(), Some("Ada"));
        tab.filter = "grace".into();
        tab.rebuild_if_needed();
        assert_eq!(tab.selected, None);
        assert_eq!(tab.scroll_to, None);
    }

    #[test]
    fn closing_a_tab_preserves_the_active_document() {
        let mut app = TabloApp {
            tabs: vec![tab(1), tab(2), tab(3)],
            active: 1,
            pending: Vec::new(),
            error: None,
            help: false,
            toast: None,
            focus_filter: false,
            next_id: 4,
        };
        app.close_tab(0);
        assert_eq!(app.current().unwrap().id, 2);
        app.close_tab(0);
        assert_eq!(app.current().unwrap().id, 3);
        app.close_tab(0);
        assert!(app.current().is_none());
        assert_eq!(app.active, 0);
        app.close_tab(99);
    }

    #[test]
    fn first_arrow_selects_first_cell_and_movement_stays_in_bounds() {
        let mut tab = tab(1);
        tab.move_sel(1, 0);
        assert_eq!(tab.selected, Some((0, 0)));
        tab.move_sel(30, 30);
        assert_eq!(tab.selected, Some((2, 1)));
        tab.move_sel(-30, -30);
        assert_eq!(tab.selected, Some((0, 0)));
        tab.filter = "absent".into();
        tab.rebuild_if_needed();
        tab.move_sel(1, 0);
        assert_eq!(tab.selected, None);
    }

    #[test]
    fn changing_headers_clears_selection_from_old_row_indices() {
        let mut tab = tab(1);
        tab.selected = Some((0, 0));
        tab.sheet_mut().set_has_header(false);
        tab.rebuild_if_needed();
        assert_eq!(tab.selected, None);
        assert_eq!(tab.filtered.len(), 4);
    }

    #[test]
    fn goto_reveals_hidden_row_and_respects_sort_order() {
        let mut tab = tab(1);
        tab.filter = "grace".into();
        tab.sort = Some((1, true));
        tab.rebuild_if_needed();
        tab.goto = true;
        tab.goto_buf = "1".into();
        tab.goto_line();
        assert!(tab.filter.is_empty());
        assert_eq!(tab.selected, Some((2, 0)));
        assert_eq!(tab.selected_text().as_deref(), Some("Ada"));
        assert!(!tab.goto);
    }
}
