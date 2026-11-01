use eframe::egui::{
    self, Align, Align2, Color32, Context, CornerRadius, FontFamily, Frame, Id, Key, Label, Layout,
    Margin, RichText, ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Ui,
};
use egui_extras::{Column, TableBuilder};
use serde_json::Value;

use crate::app::{Tab, TabloApp, ViewMode};
use crate::sheet::{self, classify_cell, CellKind, ColKind};
use crate::theme;

const ROW_H: f32 = 28.0;
const HEADER_H: f32 = 32.0;

pub fn show(app: &mut TabloApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    if let Some(tab) = app.current_mut() {
        tab.rebuild_if_needed();
    }
    handle_keys(app, &ctx);

    top_bar(app, ui);
    filter_bar(app, ui);
    if let Some(tab) = app.current_mut() {
        tab.rebuild_if_needed();
    }
    error_banner(app, ui);
    status_bar(app, ui);
    inspector(app, ui);

    egui::CentralPanel::default()
        .frame(Frame::new().fill(theme::BG).inner_margin(Margin::same(0)))
        .show(ui, |ui| {
            if app.tabs.is_empty() && app.pending.is_empty() {
                empty_state(app, ui);
            } else if app.tabs.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Chargement des données…").color(theme::MUTED));
                    });
                });
            } else if let Some(tab) = app.tabs.get_mut(app.active) {
                match tab.view {
                    ViewMode::Table => show_grid(ui, tab),
                    ViewMode::Tree => show_tree(ui, tab),
                }
            }
        });

    if app.help {
        help_window(app, &ctx);
    }
    if app.current().map(|t| t.goto).unwrap_or(false) {
        goto_window(app, &ctx);
    }
    drop_overlay(&ctx);
    toast(app, &ctx);
}

fn handle_keys(app: &mut TabloApp, ctx: &Context) {
    let wants_text = ctx.egui_wants_keyboard_input();
    let input = ctx.input(|i| {
        (
            i.modifiers.command,
            i.modifiers.shift,
            i.key_pressed(Key::O),
            i.key_pressed(Key::F),
            i.key_pressed(Key::W),
            i.key_pressed(Key::S),
            i.key_pressed(Key::R),
            i.key_pressed(Key::G),
            i.key_pressed(Key::C),
            i.key_pressed(Key::F1),
            i.key_pressed(Key::F5),
            i.key_pressed(Key::Escape),
            i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::ArrowDown),
            i.key_pressed(Key::ArrowLeft),
            i.key_pressed(Key::ArrowRight),
            i.key_pressed(Key::PageUp),
            i.key_pressed(Key::PageDown),
            i.key_pressed(Key::Home),
            i.key_pressed(Key::End),
            i.key_pressed(Key::Tab),
            i.key_pressed(Key::Enter),
            i.key_pressed(Key::Slash),
            i.key_pressed(Key::Questionmark),
        )
    });
    let (
        command,
        shift,
        o,
        f,
        w,
        s,
        r,
        g,
        c,
        f1,
        f5,
        esc,
        up,
        down,
        left,
        right,
        page_up,
        page_down,
        home,
        end,
        tab_key,
        enter,
        slash,
        question,
    ) = input;

    if command && o {
        app.open_dialog();
    }
    if command && f {
        app.focus_filter = true;
        if let Some(tab) = app.current_mut() {
            tab.view = ViewMode::Table;
        }
    }
    if command && w && !app.tabs.is_empty() {
        app.close_tab(app.active);
    }
    if command && s {
        app.export_current();
    }
    if (command && r) || f5 {
        app.reload_current();
    }
    if command && g {
        if let Some(tab) = app.current_mut() {
            tab.goto = true;
            tab.goto_buf.clear();
        }
    }
    if command && c && !wants_text {
        if let Some(tab) = app.current() {
            if shift {
                if let Some(row) = tab.selected_data_row() {
                    let text = sheet::row_as_json_string(tab.sheet(), row);
                    ctx.copy_text(text);
                    app.toast("Ligne copiée en JSON");
                }
            } else if let Some(text) = tab.selected_text() {
                ctx.copy_text(text);
                app.toast("Cellule copiée");
            }
        }
    }
    if f1 || (!wants_text && (question || (shift && slash))) {
        app.help = !app.help;
    }
    if command && tab_key && !app.tabs.is_empty() {
        if shift {
            app.active = (app.active + app.tabs.len() - 1) % app.tabs.len();
        } else {
            app.active = (app.active + 1) % app.tabs.len();
        }
    }
    if esc {
        if app.help {
            app.help = false;
        } else if let Some(tab) = app.current_mut() {
            if tab.goto {
                tab.goto = false;
            } else if !tab.filter.is_empty() {
                tab.filter.clear();
            } else {
                tab.selected = None;
            }
        }
    }
    if !wants_text && !app.help && !app.current().is_some_and(|tab| tab.goto) {
        if let Some(tab) = app.current_mut() {
            if tab.view == ViewMode::Table {
                if down {
                    tab.move_sel(1, 0);
                }
                if up {
                    tab.move_sel(-1, 0);
                }
                if left {
                    tab.move_sel(0, -1);
                }
                if right {
                    tab.move_sel(0, 1);
                }
                if page_down {
                    tab.move_sel(30, 0);
                }
                if page_up {
                    tab.move_sel(-30, 0);
                }
                if home && !tab.filtered.is_empty() {
                    let col = if command {
                        0
                    } else {
                        tab.selected.map(|(_, c)| c).unwrap_or(0)
                    };
                    tab.selected = Some((0, col));
                    tab.scroll_to = Some(0);
                }
                if end && !tab.filtered.is_empty() {
                    let last = tab.filtered.len().saturating_sub(1);
                    let col = tab.selected.map(|(_, c)| c).unwrap_or(0);
                    tab.selected = Some((last, col));
                    tab.scroll_to = Some(last);
                }
                if enter {
                    tab.show_inspector = true;
                }
            }
        }
    }
}

fn top_bar(app: &mut TabloApp, ui: &mut Ui) {
    egui::Panel::top("top")
        .frame(
            Frame::new()
                .fill(theme::SURFACE)
                .inner_margin(Margin::symmetric(12, 8))
                .stroke(Stroke::new(1.0, theme::GRID_LINE)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(
                    RichText::new("tablo")
                        .strong()
                        .size(20.0)
                        .color(Color32::WHITE),
                );
                ui.label(RichText::new("CSV · JSON").small().color(theme::MUTED));
                ui.add_space(8.0);
                if ui
                    .add(
                        egui::Button::new(RichText::new("Ouvrir").color(Color32::WHITE))
                            .fill(theme::ACCENT),
                    )
                    .on_hover_text("Ctrl+O")
                    .clicked()
                {
                    app.open_dialog();
                }
                if ui.button("Aide").on_hover_text("F1").clicked() {
                    app.help = !app.help;
                }

                if let Some(name) = app.pending.first().map(|p| p.name.clone()) {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(format!("Ouverture de {name}…")).color(theme::MUTED),
                        );
                    });
                }
            });

            if !app.tabs.is_empty() {
                ui.add_space(2.0);
                ScrollArea::horizontal()
                    .max_height(30.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let mut close = None;
                            for (i, tab) in app.tabs.iter().enumerate() {
                                let selected = i == app.active;
                                let mut text = RichText::new(&tab.opened.name);
                                if selected {
                                    text = text.strong().color(Color32::WHITE);
                                } else {
                                    text = text.color(theme::MUTED);
                                }
                                let resp = ui
                                    .selectable_label(selected, text)
                                    .on_hover_text(tab.opened.path.display().to_string());
                                if selected && !ui.clip_rect().contains_rect(resp.rect) {
                                    resp.scroll_to_me(Some(Align::Center));
                                }
                                if resp.clicked() {
                                    app.active = i;
                                }
                                if resp.middle_clicked() {
                                    close = Some(i);
                                }
                                let close_clicked = ui
                                    .push_id(("tab-x", tab.id), |ui| ui.small_button("×").clicked())
                                    .inner;
                                if close_clicked {
                                    close = Some(i);
                                }
                            }
                            if let Some(i) = close {
                                app.close_tab(i);
                            }
                        });
                    });
            }
        });
}

fn filter_bar(app: &mut TabloApp, ui: &mut Ui) {
    if app.tabs.is_empty() {
        return;
    }
    let focus = app.focus_filter;
    egui::Panel::top("filter")
        .frame(
            Frame::new()
                .fill(theme::BG)
                .inner_margin(Margin::symmetric(12, 6)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(tab) = app.current_mut() {
                    let edit = TextEdit::singleline(&mut tab.filter)
                        .id_salt(("tablo-filter", tab.id))
                        .hint_text("Rechercher…  ville:paris  -inactif")
                        .desired_width((ui.available_width() - 460.0).max(140.0));
                    let resp = ui
                        .add(edit)
                        .on_hover_text("Combine les termes avec des espaces.\nExpression : \"New York\"\nColonne : ville:\"New York\"\nExclusion : -statut:inactif\nF1 pour l’aide.");
                    if focus {
                        resp.request_focus();
                    }
                    if !tab.filter.is_empty() && ui.small_button("Effacer").clicked() {
                        tab.filter.clear();
                    }
                }

                if let Some(tab) = app.current_mut() {
                    tab.rebuild_if_needed();
                    let stats = tab_stats(tab);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut has_header = tab.sheet().has_header;
                        if ui
                            .checkbox(&mut has_header, "En-têtes")
                            .on_hover_text("Première ligne comme noms de colonnes")
                            .changed()
                        {
                            tab.sheet_mut().set_has_header(has_header);
                        }
                        if tab.is_json() {
                            ui.separator();
                            if ui
                                .selectable_label(tab.view == ViewMode::Tree, "Arbre")
                                .clicked()
                            {
                                tab.view = ViewMode::Tree;
                            }
                            if ui
                                .selectable_label(tab.view == ViewMode::Table, "Tableau")
                                .clicked()
                            {
                                tab.view = ViewMode::Table;
                            }
                        }
                        ui.separator();
                        ui.label(RichText::new(stats).color(theme::MUTED));
                    });
                }
            });
        });
    if focus {
        app.focus_filter = false;
    }
}

fn tab_stats(tab: &Tab) -> String {
    let n = tab.filtered.len();
    let total = tab.sheet().row_count();
    let cols = tab.sheet().col_count();
    if n == total {
        format!(
            "{} lignes · {} colonnes",
            sheet::fmt_int(total),
            sheet::fmt_int(cols)
        )
    } else {
        format!(
            "{} / {} lignes · {} colonnes",
            sheet::fmt_int(n),
            sheet::fmt_int(total),
            sheet::fmt_int(cols)
        )
    }
}

fn error_banner(app: &mut TabloApp, ui: &mut Ui) {
    let Some(msg) = app.error.clone() else {
        return;
    };
    egui::Panel::top("error")
        .frame(
            Frame::new()
                .fill(Color32::from_rgb(72, 24, 32))
                .inner_margin(Margin::symmetric(12, 6)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(msg).color(Color32::from_rgb(255, 210, 214)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Fermer").clicked() {
                        app.error = None;
                    }
                });
            });
        });
}

fn status_bar(app: &mut TabloApp, ui: &mut Ui) {
    egui::Panel::bottom("status")
        .frame(
            Frame::new()
                .fill(theme::SURFACE)
                .inner_margin(Margin::symmetric(12, 6))
                .stroke(Stroke::new(1.0, theme::GRID_LINE)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(tab) = app.current() {
                    kind_pill(ui, tab.opened.kind.label());
                    ui.label(
                        RichText::new(tab.opened.path.display().to_string())
                            .small()
                            .color(theme::MUTED)
                            .family(FontFamily::Monospace),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            tab.opened.encoding,
                            sheet::fmt_bytes(tab.opened.bytes)
                        ))
                        .small()
                        .color(theme::MUTED),
                    );
                    if let crate::load::DocKind::Csv { delimiter } = tab.opened.kind {
                        let delim = if delimiter == '\t' {
                            "⇥".to_string()
                        } else {
                            delimiter.to_string()
                        };
                        ui.label(
                            RichText::new(format!("séparateur {delim}"))
                                .small()
                                .color(theme::MUTED),
                        );
                    }
                    if let Some(note) = tab.opened.notes.first() {
                        ui.separator();
                        ui.label(RichText::new(note).small().color(theme::MUTED));
                    }
                    if let Some((r, c)) = tab.selected {
                        if let Some(&data_row) = tab.filtered.get(r) {
                            ui.separator();
                            ui.label(
                                RichText::new(format!(
                                    "ligne {} · {} · {}",
                                    data_row + 1,
                                    tab.sheet().header(c),
                                    tab.sheet()
                                        .kinds
                                        .get(c)
                                        .copied()
                                        .unwrap_or(ColKind::Text)
                                        .label()
                                ))
                                .small()
                                .color(theme::ACCENT),
                            );
                        }
                    }
                } else {
                    ui.label(
                        RichText::new("Dépose un fichier, ou Ctrl+O pour ouvrir. F1 pour l’aide.")
                            .color(theme::MUTED),
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .small_button("Exporter")
                        .on_hover_text("Ctrl+S — vue filtrée")
                        .clicked()
                    {
                        app.export_current();
                    }
                    if ui.small_button("Recharger").on_hover_text("F5").clicked() {
                        app.reload_current();
                    }
                });
            });
        });
}

fn inspector(app: &mut TabloApp, ui: &mut Ui) {
    let Some(tab) = app.current() else {
        return;
    };
    if !tab.show_inspector {
        return;
    }
    let (title, body, meta) = if tab.view == ViewMode::Tree {
        match &tab.tree_pick {
            Some((path, value)) => (path.clone(), value.clone(), "valeur JSON".to_string()),
            None => return,
        }
    } else {
        let Some((r, c)) = tab.selected else {
            return;
        };
        let Some(&data_row) = tab.filtered.get(r) else {
            return;
        };
        let text = tab.sheet().cell(data_row, c).to_string();
        let header = tab.sheet().header(c).to_string();
        let kind = classify_cell(&text);
        let unique = if tab.filtered.len() <= 20_000 {
            Some(tab.sheet().unique_count(c, &tab.filtered))
        } else {
            None
        };
        let meta = match unique {
            Some(n) => format!(
                "{} · {} valeurs uniques dans la vue",
                cell_kind_label(kind),
                sheet::fmt_int(n)
            ),
            None => cell_kind_label(kind).to_string(),
        };
        (header, text, meta)
    };

    egui::Panel::bottom("inspector")
        .resizable(true)
        .default_size(150.0)
        .min_size(88.0)
        .frame(
            Frame::new()
                .fill(theme::SURFACE)
                .inner_margin(Margin::symmetric(12, 8))
                .stroke(Stroke::new(1.0, theme::GRID_LINE)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&title).strong().color(theme::ACCENT));
                ui.label(RichText::new(meta).small().color(theme::MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Masquer").clicked() {
                        if let Some(tab) = app.current_mut() {
                            tab.show_inspector = false;
                        }
                    }
                    if ui.button("Copier").clicked() {
                        ui.ctx().copy_text(body.clone());
                        app.toast("Copié");
                    }
                    if let Some(pretty) = sheet::pretty_json_if_possible(&body) {
                        if ui.button("Copier pretty JSON").clicked() {
                            ui.ctx().copy_text(pretty);
                            app.toast("JSON formaté copié");
                        }
                    }
                });
            });
            ui.add_space(4.0);
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let shown =
                        sheet::pretty_json_if_possible(&body).unwrap_or_else(|| body.clone());
                    ui.add(
                        Label::new(
                            RichText::new(shown)
                                .family(FontFamily::Monospace)
                                .size(13.5)
                                .color(theme::TEXT),
                        )
                        .wrap(),
                    );
                });
        });
}

fn show_grid(ui: &mut Ui, tab: &mut Tab) {
    if tab.filtered.is_empty() {
        ui.centered_and_justified(|ui| {
            if tab.sheet().row_count() == 0 {
                ui.label(
                    RichText::new("Fichier vide.")
                        .color(theme::MUTED)
                        .size(18.0),
                );
            } else {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(format!("Aucune ligne ne correspond à « {} »", tab.filter))
                            .color(theme::TEXT)
                            .size(18.0),
                    );
                    if ui.button("Effacer le filtre").clicked() {
                        tab.filter.clear();
                    }
                });
            }
        });
        return;
    }

    let headers = tab.sheet().headers.clone();
    let kinds = tab.sheet().kinds.clone();
    let widths = tab.sheet().widths.clone();
    let cols = headers.len();
    if cols == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new("Pas de colonnes à afficher.").color(theme::MUTED));
        });
        return;
    }

    // TableBuilder only scrolls vertically. Keep its header and body together
    // in an outer horizontal area so trackpad gestures reach both axes.
    ScrollArea::horizontal()
        .id_salt(("grid-horizontal", tab.id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_grid_contents(ui, tab, &headers, &kinds, &widths);
        });
}

fn show_grid_contents(
    ui: &mut Ui,
    tab: &mut Tab,
    headers: &[String],
    kinds: &[ColKind],
    widths: &[f32],
) {
    let filtered_len = tab.filtered.len();
    let mut builder = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .cell_layout(Layout::left_to_right(Align::Center))
        .sense(Sense::click())
        .min_scrolled_height(0.0)
        .auto_shrink([false, false])
        .id_salt(("grid", tab.id));

    if let Some(row) = tab.scroll_to.take() {
        builder = builder.scroll_to_row(row, Some(Align::Center));
    }

    builder = builder.column(Column::exact(64.0));
    for i in 0..headers.len() {
        let width = widths.get(i).copied().unwrap_or(140.0);
        let col = Column::initial(width)
            .clip(true)
            .resizable(true)
            .at_least(64.0)
            .at_most(560.0);
        builder = builder.column(col);
    }

    let sort = tab.sort;
    let selected = tab.selected;

    builder
        .header(HEADER_H, |mut header| {
            header.col(|ui| {
                ui.label(RichText::new("#").small().color(theme::MUTED));
            });
            for (col, name) in headers.iter().enumerate() {
                header.col(|ui| {
                    let kind = kinds.get(col).copied().unwrap_or(ColKind::Text);
                    let arrow = match sort {
                        Some((c, true)) if c == col => "  ▲",
                        Some((c, false)) if c == col => "  ▼",
                        _ => "",
                    };
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                        ui.painter()
                            .circle_filled(rect.center(), 3.5, theme::kind_color(kind));
                        let label = format!("{name}{arrow}");
                        let resp = ui.add(
                            Label::new(RichText::new(label).strong().color(theme::TEXT))
                                .sense(Sense::click()),
                        );
                        if resp.clicked() {
                            tab.sort = match tab.sort {
                                Some((c, true)) if c == col => Some((col, false)),
                                Some((c, false)) if c == col => None,
                                _ => Some((col, true)),
                            };
                        }
                        resp.on_hover_text(format!("{name} · {}", kind.label()));
                    });
                });
            }
        })
        .body(|body| {
            body.rows(ROW_H, filtered_len, |mut row| {
                let i = row.index();
                let data_row = tab.filtered[i];
                let is_row_sel = selected.map(|(r, _)| r) == Some(i);
                row.set_selected(is_row_sel);

                row.col(|ui| {
                    ui.label(
                        RichText::new((data_row + 1).to_string())
                            .small()
                            .color(theme::MUTED)
                            .family(FontFamily::Monospace),
                    );
                });

                for (col, header_name) in headers.iter().enumerate() {
                    let raw = tab.sheet().cell(data_row, col).to_string();
                    row.col(|ui| {
                        let shown = sheet::truncate(&raw, 140);
                        let is_cell = selected == Some((i, col));
                        let mut text = RichText::new(shown.as_ref()).color(theme::cell_color(&raw));
                        if matches!(
                            classify_cell(&raw),
                            CellKind::Integer | CellKind::Float | CellKind::Json
                        ) {
                            text = text.family(FontFamily::Monospace);
                        }
                        if is_cell {
                            ui.painter().rect_filled(
                                ui.max_rect(),
                                CornerRadius::same(4),
                                Color32::from_rgba_unmultiplied(79, 169, 241, 50),
                            );
                        }
                        let resp = ui.add(Label::new(text).sense(Sense::click()).truncate());
                        if resp.clicked() || resp.double_clicked() {
                            tab.selected = Some((i, col));
                            tab.show_inspector = true;
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Copier la cellule").clicked() {
                                ui.ctx().copy_text(raw.clone());
                                ui.close();
                            }
                            if ui.button("Copier la ligne (JSON)").clicked() {
                                ui.ctx()
                                    .copy_text(sheet::row_as_json_string(tab.sheet(), data_row));
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    !raw.is_empty(),
                                    egui::Button::new("Filtrer cette valeur"),
                                )
                                .clicked()
                            {
                                tab.filter = sheet::column_query(header_name, &raw);
                                ui.close();
                            }
                        });
                    });
                }
            });
        });
}

fn show_tree(ui: &mut Ui, tab: &mut Tab) {
    let Some(value) = tab.opened.json.clone() else {
        ui.label("Pas de JSON dans ce document.");
        return;
    };
    Frame::new()
        .fill(theme::BG)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    show_value(ui, "racine", &value, "", tab);
                });
        });
}

fn show_value(ui: &mut Ui, key: &str, value: &Value, path: &str, tab: &mut Tab) {
    match value {
        Value::Object(map) => {
            let title = format!("{key}   {{ {} }}", map.len());
            let id = ui.make_persistent_id(("obj", path));
            let default_open = path.is_empty();
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                default_open,
            )
            .show_header(ui, |ui| {
                ui.label(RichText::new(title).color(theme::GOLD).strong());
            })
            .body(|ui| {
                for (k, v) in map {
                    let child = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    show_value(ui, k, v, &child, tab);
                }
            });
        }
        Value::Array(arr) => {
            let title = format!("{key}   [ {} ]", arr.len());
            let id = ui.make_persistent_id(("arr", path));
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                path.is_empty(),
            )
            .show_header(ui, |ui| {
                ui.label(RichText::new(title).color(theme::PURPLE).strong());
            })
            .body(|ui| {
                let cap = arr.len().min(300);
                for (i, v) in arr.iter().take(cap).enumerate() {
                    let child = if path.is_empty() {
                        format!("[{i}]")
                    } else {
                        format!("{path}[{i}]")
                    };
                    show_value(ui, &i.to_string(), v, &child, tab);
                }
                if arr.len() > cap {
                    ui.label(
                        RichText::new(format!("… {} éléments de plus", arr.len() - cap))
                            .color(theme::MUTED)
                            .italics(),
                    );
                }
            });
        }
        _ => {
            ui.horizontal(|ui| {
                let key_resp = ui.add(
                    Label::new(
                        RichText::new(key)
                            .color(theme::MUTED)
                            .family(FontFamily::Monospace),
                    )
                    .sense(Sense::click()),
                );
                ui.label(RichText::new("·").color(theme::GRID_LINE));
                let rendered = display_json_scalar(value);
                let resp = ui.add(
                    Label::new(
                        RichText::new(sheet::truncate(&rendered, 160).as_ref())
                            .color(theme::cell_color(&rendered))
                            .family(FontFamily::Monospace),
                    )
                    .sense(Sense::click()),
                );
                let combined = key_resp.union(resp);
                if combined.clicked() {
                    tab.tree_pick = Some((path.to_string(), rendered.clone()));
                    tab.show_inspector = true;
                }
                combined.context_menu(|ui| {
                    if ui.button("Copier la valeur").clicked() {
                        ui.ctx().copy_text(rendered.clone());
                        ui.close();
                    }
                    if ui.button("Copier le chemin").clicked() {
                        ui.ctx().copy_text(path.to_string());
                        ui.close();
                    }
                });
            });
        }
    }
}

fn display_json_scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn empty_state(app: &mut TabloApp, ui: &mut Ui) {
    let rect = ui.available_rect_before_wrap().shrink(28.0);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(16),
        Stroke::new(1.4, theme::GRID_LINE),
        StrokeKind::Inside,
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label(
                    RichText::new("tablo")
                        .size(36.0)
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.label(
                    RichText::new("Lecteur CSV et JSON, léger et rapide")
                        .size(16.0)
                        .color(theme::MUTED),
                );
                ui.add_space(16.0);
                ui.label(
                    RichText::new("Dépose un fichier ici, ou ouvre-en un.")
                        .size(18.0)
                        .color(theme::TEXT),
                );
                ui.add_space(10.0);
                if ui
                    .add_sized(
                        [210.0, 40.0],
                        egui::Button::new(
                            RichText::new("Ouvrir un fichier  ·  Ctrl+O").color(theme::BG),
                        )
                        .fill(theme::ACCENT),
                    )
                    .clicked()
                {
                    app.open_dialog();
                }
                ui.add_space(6.0);
                if ui.button("Essayer avec des exemples").clicked() {
                    app.queue_bytes(
                        "exemple.csv".into(),
                        include_bytes!("../samples/people.csv").to_vec(),
                    );
                    app.queue_bytes(
                        "exemple.json".into(),
                        include_bytes!("../samples/products.json").to_vec(),
                    );
                }
                ui.add_space(16.0);
                ui.label(
                    RichText::new("tablo fichier.csv    ·    tablo data.json")
                        .family(FontFamily::Monospace)
                        .color(theme::MUTED),
                );
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Ctrl+O ouvrir   ·   Ctrl+F filtrer   ·   F1 aide")
                        .small()
                        .color(theme::MUTED),
                );
            });
        });
    });
}

fn help_window(app: &mut TabloApp, ctx: &Context) {
    egui::Window::new("Raccourcis")
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(
            Frame::new()
                .fill(theme::SURFACE)
                .corner_radius(12)
                .inner_margin(Margin::same(16))
                .stroke(Stroke::new(1.0, theme::GRID_LINE)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(460.0);
            egui::Grid::new("help-grid")
                .num_columns(2)
                .spacing([24.0, 8.0])
                .show(ui, |ui| {
                    help_row(ui, "Ctrl+O", "Ouvrir un fichier");
                    help_row(ui, "Glisser-déposer", "Ouvrir un ou plusieurs fichiers");
                    help_row(ui, "Ctrl+F", "Filtrer les lignes");
                    help_row(ui, "colonne:valeur", "Filtre sur une colonne");
                    help_row(ui, "\"New York\"", "Rechercher une expression");
                    help_row(ui, "ville:\"New York\"", "Expression dans une colonne");
                    help_row(ui, "-statut:inactif", "Exclure les lignes correspondantes");
                    help_row(
                        ui,
                        "\"Nom complet\":ada",
                        "Colonne dont le nom contient des espaces",
                    );
                    help_row(ui, "Clic en-tête", "Trier (asc / desc / off)");
                    help_row(ui, "Flèches", "Déplacer la sélection");
                    help_row(ui, "Ctrl+C", "Copier la cellule");
                    help_row(ui, "Ctrl+Shift+C", "Copier la ligne en JSON");
                    help_row(ui, "Ctrl+G", "Aller à la ligne");
                    help_row(ui, "Ctrl+S", "Exporter la vue filtrée");
                    help_row(ui, "F5 / Ctrl+R", "Recharger");
                    help_row(ui, "Ctrl+W", "Fermer l’onglet");
                    help_row(ui, "Ctrl+Tab", "Onglet suivant");
                    help_row(ui, "F1", "Afficher cette aide");
                    help_row(ui, "Échap", "Fermer / vider le filtre");
                });
            ui.add_space(12.0);
            if ui.button("Fermer").clicked() {
                app.help = false;
            }
        });
}

fn help_row(ui: &mut Ui, key: &str, meaning: &str) {
    ui.label(
        RichText::new(key)
            .family(FontFamily::Monospace)
            .color(theme::ACCENT),
    );
    ui.label(RichText::new(meaning).color(theme::TEXT));
    ui.end_row();
}

fn goto_window(app: &mut TabloApp, ctx: &Context) {
    let mut open = true;
    egui::Window::new("Aller à la ligne")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let max = app.current().map(|t| t.sheet().row_count()).unwrap_or(0);
            ui.label(format!("Numéro entre 1 et {}", sheet::fmt_int(max)));
            let mut go = false;
            if let Some(tab) = app.current_mut() {
                let resp = ui.add(
                    TextEdit::singleline(&mut tab.goto_buf)
                        .hint_text("142")
                        .desired_width(180.0),
                );
                resp.request_focus();
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    go = true;
                }
            }
            if ui.button("Aller").clicked() {
                go = true;
            }
            if go {
                if let Some(tab) = app.current_mut() {
                    tab.goto_line();
                }
            }
        });
    if !open {
        if let Some(tab) = app.current_mut() {
            tab.goto = false;
        }
    }
}

fn drop_overlay(ctx: &Context) {
    let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
    if !hovering {
        return;
    }
    egui::Area::new(Id::unique("drop"))
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_rgba_unmultiplied(13, 16, 21, 210))
                .corner_radius(16)
                .inner_margin(Margin::same(28))
                .stroke(Stroke::new(1.5, theme::ACCENT))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Relâche pour ouvrir")
                            .size(22.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                });
        });
}

fn toast(app: &mut TabloApp, ctx: &Context) {
    let now = ctx.input(|i| i.time);
    let mut keep = true;
    if let Some(t) = &mut app.toast {
        if t.until.is_none() {
            t.until = Some(now + 1.8);
        }
        if t.until.map(|u| now > u).unwrap_or(false) {
            keep = false;
        }
    } else {
        return;
    }
    if !keep {
        app.toast = None;
        return;
    }
    let text = app.toast.as_ref().unwrap().text.clone();
    egui::Area::new(Id::unique("toast"))
        .anchor(Align2::CENTER_BOTTOM, [0.0, -28.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            Frame::new()
                .fill(theme::LIFT)
                .corner_radius(10)
                .inner_margin(Margin::symmetric(16, 8))
                .stroke(Stroke::new(1.0, theme::ACCENT))
                .show(ui, |ui| {
                    ui.label(RichText::new(text).color(Color32::WHITE));
                });
        });
}

fn kind_pill(ui: &mut Ui, kind: &str) {
    let (bg, fg) = theme::pill(kind);
    Frame::new()
        .fill(bg)
        .corner_radius(6)
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(kind).small().strong().color(fg));
        });
}

fn cell_kind_label(kind: CellKind) -> &'static str {
    match kind {
        CellKind::Empty => "vide",
        CellKind::Bool => "booléen",
        CellKind::Integer => "entier",
        CellKind::Float => "nombre",
        CellKind::Date => "date",
        CellKind::Json => "json",
        CellKind::Text => "texte",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load;
    use std::path::PathBuf;

    #[test]
    fn wide_grid_scrolls_both_ways_with_trackpad_events() {
        let headers = (0..12).map(|i| format!("colonne_{i}")).collect::<Vec<_>>();
        let mut csv = format!("{}\n", headers.join(","));
        for row in 0..100 {
            let values = (0..12).map(|col| format!("cellule-{row}-{col}"));
            csv.push_str(&values.collect::<Vec<_>>().join(","));
            csv.push('\n');
        }
        let opened = load::open_bytes(
            PathBuf::from("large.csv"),
            "large.csv".into(),
            csv.as_bytes(),
        )
        .unwrap();
        let mut tab = Tab::new(1, opened);
        let ctx = Context::default();
        let mut frame = 0;
        let mut render = |delta: egui::Vec2| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 400.0),
                )),
                time: Some(frame as f64 / 60.0),
                ..Default::default()
            };
            frame += 1;
            input
                .events
                .push(egui::Event::PointerMoved(egui::pos2(300.0, 150.0)));
            if delta != egui::Vec2::ZERO {
                input.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta,
                    phase: egui::TouchPhase::Move,
                    // Linux does not distinguish trackpads from mouse wheels.
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut scroll_id = None;
            let mut output = ctx.run_ui(input, |ui| {
                scroll_id =
                    Some(ui.make_persistent_id(egui::IdSalt::new(("grid-horizontal", tab.id))));
                show_grid(ui, &mut tab);
            });
            // This test exercises layout and input without a GPU renderer.
            output.textures_delta.clear();
            egui::scroll_area::State::load(&ctx, scroll_id.unwrap())
                .unwrap()
                .clamped_offset()
        };
        render(egui::Vec2::ZERO);
        assert_eq!(render(egui::Vec2::ZERO).x, 0.0);
        let right = render(egui::vec2(-6.0, 0.0));
        assert!(
            right.x > 0.0,
            "The horizontal gesture must move the grid: {right:?}"
        );
        let left = render(egui::vec2(4.0, 0.0));
        assert!(
            left.x < right.x,
            "The opposite gesture must move back: {left:?}"
        );
    }
}
