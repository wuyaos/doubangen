#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod common;
pub mod doubaninfo;
pub mod tmdb;
pub mod schema;
pub mod config;

use crate::common::Candidate;
use crate::config::Config;
use eframe::egui;
use serde_json::Value;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread;

// ───────── 按钮统一样式 ─────────

/// 加载窗口图标（PNG → RGBA IconData）
fn load_icon() -> std::sync::Arc<egui::IconData> {
    let bytes = include_bytes!("../assets/icon.png");
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            std::sync::Arc::new(egui::IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            })
        }
        Err(_) => std::sync::Arc::new(egui::IconData::default()),
    }
}

/// 下拉选项中文映射（值保持英文用于逻辑判断）
fn cn_source(v: &str) -> &'static str {
    match v { "auto" => "自动", "douban" => "豆瓣", "tmdb" => "TMDB", "merge" => "聚合", _ => "" }
}
fn cn_format(v: &str) -> &'static str {
    match v { "bbcode" => "BBCode", "json" => "JSON", _ => "" }
}
fn cn_media(v: &str) -> &'static str {
    match v { "auto" => "自动", "movie" => "电影", "tv" => "剧集", _ => "" }
}

fn primary_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(
        egui::RichText::new(text).color(egui::Color32::from_rgb(0x1e, 0x1e, 0x2e)).strong()
    ).fill(egui::Color32::from_rgb(0xF5, 0xA6, 0x23)))
}

fn secondary_btn(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(text).fill(egui::Color32::from_rgb(0x45, 0x47, 0x5a)))
}

// ───────── main ─────────

fn main() -> eframe::Result {
    let cfg = find_config_path()
        .and_then(|p| config::load(&p).ok().map(|c| (c, p)))
        .map(|(c, p)| (Arc::new(Mutex::new(c)), p))
        .unwrap_or_else(|| (Arc::new(Mutex::new(Config::default())), "config.toml".into()));
    let icon = load_icon();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "豆瓣简介生成器",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            set_mocha_theme(&cc.egui_ctx);
            install_system_font(&cc.egui_ctx);
            Ok(Box::new(App::new(cfg.0, cfg.1)))
        }),
    )
}

// ───────── 状态 ─────────

enum RightState {
    Empty,
    Loading(String),
    Result { bbcode: String, json: String },
    Error(String),
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct HistoryItem {
    pub title: String,
    pub id: String,
    pub source: String,
    pub format: String,
    pub content: String,
    pub year: String,
    pub timestamp: String,
}

enum WorkerMsg {
    Cands(Vec<Candidate>),
    Data(Value),
    Err(String),
}

struct App {
    cfg: Arc<Mutex<Config>>,
    cfg_path: String,
    input: String,
    source: String,
    format: String,
    media_type: String,
    full_celebrities: bool,
    cands: Vec<Candidate>,
    filter: String,
    selected_idx: Option<usize>,
    right: RightState,
    rx: Option<Receiver<WorkerMsg>>,
    usage_hint: String,
    status_toast: String,
    settings_open: bool,
    settings_draft: Config,
    searching: bool,
    first_focus: bool,
    history: Vec<HistoryItem>,
    history_path: String,
    filter_cat: String,
    filter_year: String,
}

impl App {
    fn new(cfg: Arc<Mutex<Config>>, cfg_path: String) -> Self {
        let (source, format, media_type, full_celebrities, keys_empty) = {
            let c = cfg.lock().unwrap();
            (
                c.defaults.source.clone(),
                c.defaults.format.clone(),
                c.defaults.media_type.clone(),
                c.defaults.full_celebrities,
                c.keys.doubaninfo.is_empty() && c.keys.tmdb.is_empty(),
            )
        };
        let right = if keys_empty {
            RightState::Error("未找到 config.toml，点右上角 ⚙ 设置填入 API key".into())
        } else {
            RightState::Empty
        };
        let history_path = history_path_for(&cfg_path);
        let mut app = Self {
            cfg,
            cfg_path,
            input: String::new(),
            source,
            format,
            media_type,
            full_celebrities,
            cands: Vec::new(),
            filter: String::new(),
            selected_idx: None,
            right,
            rx: None,
            usage_hint: String::new(),
            status_toast: String::new(),
            settings_open: false,
            settings_draft: Config::default(),
            searching: false,
            first_focus: true,
            history: Vec::new(),
            history_path,
            filter_cat: "全部".into(),
            filter_year: String::new(),
        };
        app.history = load_history(&app.history_path);
        app
    }

    fn fetch(&mut self, ctx: &egui::Context) {
        if self.input.trim().is_empty() {
            self.right = RightState::Error("请输入片名 / 豆瓣ID / IMDb / TMDB URL".into());
            return;
        }
        let (tx, rx) = channel::<WorkerMsg>();
        self.rx = Some(rx);
        self.right = RightState::Empty;
        self.searching = true;
        self.cands.clear();
        self.selected_idx = None;
        let cfg = self.cfg.clone();
        let input = self.input.clone();
        let source = self.source.clone();
        let media_type = self.media_type.clone();
        let fc = self.full_celebrities;
        let ctx2 = ctx.clone();
        thread::spawn(move || {
            let c = cfg.lock().unwrap().clone();
            let r = phase1(&c, &input, &source, &media_type, fc);
            match r {
                Ok(Outcome::Data(d)) => { let _ = tx.send(WorkerMsg::Data(d)); }
                Ok(Outcome::Cands(cs)) => { let _ = tx.send(WorkerMsg::Cands(cs)); }
                Err(e) => { let _ = tx.send(WorkerMsg::Err(e)); }
            }
            ctx2.request_repaint();
        });
    }

    fn select_candidate(&mut self, idx: usize, ctx: &egui::Context) {
        let cand = match self.cands.get(idx).cloned() {
            Some(c) => c,
            None => return,
        };
        self.selected_idx = Some(idx);
        let title = if cand.chinese_title.is_empty() { cand.title.clone() } else { cand.chinese_title.clone() };
        let (tx, rx) = channel::<WorkerMsg>();
        self.rx = Some(rx);
        self.right = RightState::Loading(format!("正在取《{}》详情…", title));
        let cfg = self.cfg.clone();
        let source = self.source.clone();
        let fc = self.full_celebrities;
        let ctx2 = ctx.clone();
        thread::spawn(move || {
            let c = cfg.lock().unwrap().clone();
            match phase2(&c, &cand, &source, fc) {
                Ok(d) => { let _ = tx.send(WorkerMsg::Data(d)); }
                Err(e) => { let _ = tx.send(WorkerMsg::Err(e)); }
            }
            ctx2.request_repaint();
        });
    }

    fn poll(&mut self) {
        let rx = match &self.rx { Some(r) => r, None => return };
        if let Ok(msg) = rx.try_recv() {
            match msg {
                WorkerMsg::Cands(cs) => {
                    self.cands = cs;
                    self.searching = false;
                    if self.cands.is_empty() {
                        let q = self.input.trim();
                        self.right = RightState::Error(format!(
                            "未找到「{}」，试试加年份如「{} 2025」",
                            q, q
                        ));
                    } else {
                        self.right = RightState::Empty;
                    }
                }
                WorkerMsg::Data(d) => {
                    let usage = d.get("usage").cloned();
                    self.extract_usage(usage);
                    // 先提取历史字段（借用 d），再 clean_output（move d）
                    let title = d.get("chinese_title").and_then(|v| v.as_str())
                        .or_else(|| d.get("title").and_then(|v| v.as_str())).unwrap_or("").to_string();
                    let id = d.get("sid").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let src = d.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let year = d.get("year").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let bbcode = d.get("format").and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| schema::to_bbcode(&d));
                    let json = serde_json::to_string_pretty(&schema::clean_output(d)).unwrap_or_default();
                    let content = if self.format == "bbcode" { bbcode.clone() } else { json.clone() };
                    let item = HistoryItem {
                        title, id, source: src, format: self.format.clone(), content, year,
                        timestamp: chrono_now(),
                    };
                    self.history.insert(0, item);
                    if self.history.len() > 100 { self.history.truncate(100); }
                    let _ = save_history(&self.history_path, &self.history);
                    self.right = RightState::Result { bbcode, json };
                }
                WorkerMsg::Err(e) => self.right = RightState::Error(e),
            }
            self.rx = None;
        }
    }

    fn extract_usage(&mut self, usage: Option<Value>) {
        if let Some(u) = usage {
            if let Some(used) = u.get("used").and_then(|v| v.as_object()) {
                let kd = used.get("key_day").and_then(|v| v.as_u64()).unwrap_or(0);
                let klim = u.get("limits").and_then(|v| v.get("key_per_day")).and_then(|v| v.as_u64()).unwrap_or(0);
                self.usage_hint = format!("豆瓣配额 {}/{}", kd, klim);
            }
        }
    }

    fn save_settings(&mut self) -> bool {
        let draft = self.settings_draft.clone();
        let toml_str = match toml::to_string(&draft) {
            Ok(s) => s,
            Err(e) => {
                self.status_toast = format!("配置格式错误：{}", e);
                return false;
            }
        };
        if let Err(e) = std::fs::write(&self.cfg_path, toml_str) {
            self.status_toast = format!("保存失败：{}", e);
            return false;
        }
        *self.cfg.lock().unwrap() = draft.clone();
        self.source = draft.defaults.source.clone();
        self.format = draft.defaults.format.clone();
        self.media_type = draft.defaults.media_type.clone();
        self.full_celebrities = draft.defaults.full_celebrities;
        self.status_toast = format!("已保存到 {}", self.cfg_path);
        true
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        // 设置窗口
        let mut open = self.settings_open;
        let mut do_save = false;
        let mut do_close = false;
        if open {
            egui::Window::new("⚙ 设置").open(&mut open).resizable(true).min_width(380.0).default_width(440.0).show(ctx, |ui| {
                // API Keys
                ui.add(egui::Label::new(egui::RichText::new("API Keys").strong()));
                ui.horizontal(|ui| {
                    ui.add_sized([85.0, 18.0], egui::Label::new("DoubanInfo:"));
                    ui.add(egui::TextEdit::singleline(&mut self.settings_draft.keys.doubaninfo).desired_width(ui.available_width() - 8.0));
                });
                ui.horizontal(|ui| {
                    ui.add_sized([85.0, 18.0], egui::Label::new("TMDB:"));
                    ui.add(egui::TextEdit::singleline(&mut self.settings_draft.keys.tmdb).desired_width(ui.available_width() - 8.0));
                });
                ui.separator();
                // 默认行为
                ui.add(egui::Label::new(egui::RichText::new("默认行为").strong()));
                ui.horizontal(|ui| {
                    ui.add_sized([85.0, 18.0], egui::Label::new("源:"));
                    egui::ComboBox::from_id_salt("s_src").width(120.0).selected_text(cn_source(&self.settings_draft.defaults.source))
                        .show_ui(ui, |ui| { for (v,l) in [("auto","自动"),("douban","豆瓣"),("tmdb","TMDB"),("merge","聚合")] { ui.selectable_value(&mut self.settings_draft.defaults.source, v.into(), l); } });
                });
                ui.horizontal(|ui| {
                    ui.add_sized([85.0, 18.0], egui::Label::new("格式:"));
                    egui::ComboBox::from_id_salt("s_fmt").width(120.0).selected_text(cn_format(&self.settings_draft.defaults.format))
                        .show_ui(ui, |ui| { for (v,l) in [("bbcode","BBCode"),("json","JSON")] { ui.selectable_value(&mut self.settings_draft.defaults.format, v.into(), l); } });
                });
                ui.horizontal(|ui| {
                    ui.add_sized([85.0, 18.0], egui::Label::new("类型:"));
                    egui::ComboBox::from_id_salt("s_mt").width(120.0).selected_text(cn_media(&self.settings_draft.defaults.media_type))
                        .show_ui(ui, |ui| { for (v,l) in [("auto","自动"),("movie","电影"),("tv","剧集")] { ui.selectable_value(&mut self.settings_draft.defaults.media_type, v.into(), l); } });
                    ui.checkbox(&mut self.settings_draft.defaults.full_celebrities, "详细");
                });
                ui.separator();
                // 限流
                ui.add(egui::Label::new(egui::RichText::new("限流").strong()));
                ui.horizontal(|ui| {
                    ui.add_sized([70.0, 18.0], egui::Label::new("重试:"));
                    ui.add(egui::DragValue::new(&mut self.settings_draft.limits.retry));
                    ui.add_sized([70.0, 18.0], egui::Label::new("超时:"));
                    ui.add(egui::DragValue::new(&mut self.settings_draft.limits.timeout));
                    ui.add_sized([70.0, 18.0], egui::Label::new("间隔:"));
                    ui.add(egui::DragValue::new(&mut self.settings_draft.limits.min_interval));
                });
                ui.separator();
                // 底部按钮：✕ 关闭 在右下角（right_to_left 布局）
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if secondary_btn(ui, "× 关闭").clicked() { do_close = true; }
                    if primary_btn(ui, "💾 保存").clicked() { do_save = true; }
                });
            });
        }
        if do_close { open = false; }
        if do_save && self.save_settings() {
            open = false;
        }
        self.settings_open = open;

        // 顶部面板（去标题，一行）
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().interact_size.y = 22.0;
                ui.label("源:");
                egui::ComboBox::from_id_salt("source").width(90.0).selected_text(cn_source(&self.source))
                    .show_ui(ui, |ui| { for (v,l) in [("auto","自动"),("douban","豆瓣"),("tmdb","TMDB"),("merge","聚合")] { ui.selectable_value(&mut self.source, v.into(), l); } });
                let resp = ui.add_sized([ui.available_width() - 150.0, 20.0],
                    egui::TextEdit::singleline(&mut self.input)
                        .hint_text("标题 / 豆瓣ID / IMDb(tt…) / 豆瓣链接 / TMDB URL")
                        .desired_width(f32::MAX));
                if self.first_focus { resp.request_focus(); self.first_focus = false; }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.fetch(ctx);
                    resp.request_focus();
                }
                if primary_btn(ui, "🔍 搜索").clicked() { self.fetch(ctx); }
                if secondary_btn(ui, "⚙").clicked() {
                    self.settings_draft = self.cfg.lock().unwrap().clone();
                    self.settings_open = true;
                }
            });
            ui.add_space(4.0);
        });

        // 左侧海报墙
        egui::SidePanel::left("posters").exact_width(380.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("搜索结果").strong());
                ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("标题关键字…").desired_width(110.0));
            });
            // 分类 chip
            ui.horizontal(|ui| {
                for cat in ["全部","动漫","剧集","电影"] {
                    if ui.selectable_label(self.filter_cat == cat, cat).clicked() { self.filter_cat = cat.into(); }
                }
            });
            // 年份下拉
            ui.horizontal(|ui| {
                ui.label("年份:");
                let mut years: Vec<String> = self.cands.iter().filter(|c| !c.year.is_empty()).map(|c| c.year.clone()).collect();
                years.sort(); years.dedup();
                egui::ComboBox::from_id_salt("year").width(90.0).selected_text(if self.filter_year.is_empty() { "全部" } else { &self.filter_year })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.filter_year, String::new(), "全部");
                        for y in &years { ui.selectable_value(&mut self.filter_year, y.clone(), y); }
                    });
            });
            ui.separator();
            if self.searching {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.spinner();
                    ui.label("搜索中…");
                });
            } else if self.cands.is_empty() {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("🎬 输入片名/链接，开始查询").color(egui::Color32::from_rgb(0xa6,0xad,0xc8)).size(13.0));
                });
            } else {
                let filter = self.filter.to_lowercase();
                let filtered: Vec<(usize, &Candidate)> = self.cands.iter().enumerate().filter(|(_, c)| {
                    if !filter.is_empty() {
                        let matches = c.title.to_lowercase().contains(&filter)
                            || c.chinese_title.to_lowercase().contains(&filter)
                            || c.year.contains(&filter)
                            || c.region.to_lowercase().contains(&filter)
                            || c.genre.iter().any(|g| g.to_lowercase().contains(&filter));
                        if !matches { return false; }
                    }
                    if self.filter_cat == "动漫" && !c.genre.iter().any(|g| g.contains("动画") || g.contains("Anime") || g.contains("アニメ")) { return false; }
                    if self.filter_cat == "剧集" && c.media_type != "tv" { return false; }
                    if self.filter_cat == "电影" && c.media_type != "movie" { return false; }
                    if !self.filter_year.is_empty() && c.year != self.filter_year { return false; }
                    true
                }).collect();
                let clicked: std::cell::Cell<Option<usize>> = std::cell::Cell::new(None);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.vertical_centered(|ui| {
                        let avail: f32 = 380.0;
                        let max_w = 110.0;
                        let spacing = 8.0;
                        let cols = (((avail + spacing) / (max_w + spacing)).floor() as usize).max(1);
                        let cell_w = max_w;
                        egui::Grid::new("poster_wall").num_columns(cols).spacing([spacing, spacing]).show(ui, |ui| {
                            for (col, (orig_idx, c)) in filtered.iter().enumerate() {
                                poster_tile(ui, c, cell_w, &clicked, *orig_idx, self.selected_idx == Some(*orig_idx));
                                if (col + 1) % cols == 0 { ui.end_row(); }
                            }
                        });
                    });
                });
                if let Some(i) = clicked.get() {
                    self.select_candidate(i, ctx);
                }
            }
        });

        // 右侧：上部结果 + 下部历史
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut new_fmt = self.format.clone();
            let mut do_copy = false;
            let mut do_back = false;
            ui.push_id("result_area", |ui| {
                match &self.right {
                    RightState::Empty => {
                        ui.add_space(60.0);
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("点击左侧海报查看详情").color(egui::Color32::from_rgb(0xa6,0xad,0xc8)).size(14.0));
                        });
                    }
                    RightState::Loading(msg) => {
                        ui.add_space(60.0);
                        ui.vertical_centered(|ui| {
                            ui.spinner();
                            ui.label(msg.as_str());
                        });
                    }
                    RightState::Error(msg) => {
                        ui.add_space(40.0);
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new(msg.as_str()).color(egui::Color32::from_rgb(0xf3,0x8b,0xa8)).size(13.0));
                            ui.add_space(8.0);
                            if secondary_btn(ui, "重试").clicked() { do_back = true; }
                        });
                    }
                    RightState::Result { bbcode, json } => {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().interact_size.y = 20.0;
                            ui.label("格式:");
                            egui::ComboBox::from_id_salt("r_fmt").width(90.0).selected_text(cn_format(&new_fmt))
                                .show_ui(ui, |ui| { for (v,l) in [("bbcode","BBCode"),("json","JSON")] { ui.selectable_value(&mut new_fmt, v.into(), l); } });
                            if primary_btn(ui, "📋 复制").clicked() { do_copy = true; }
                        });
                        ui.separator();
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let text = if new_fmt == "bbcode" { bbcode } else { json };
                            ui.add(egui::Label::new(egui::RichText::new(text.as_str()).monospace()).selectable(true));
                        });
                    }
                }
                self.format = new_fmt;
                if do_copy {
                    if let RightState::Result { bbcode, json } = &self.right {
                        let text = if self.format == "bbcode" { bbcode.clone() } else { json.clone() };
                        ui.ctx().copy_text(text);
                        self.status_toast = "已复制".into();
                    }
                }
                if do_back { self.right = RightState::Empty; }
            });
            ui.separator();
            // 右下历史折叠
            let mut clicked_hist: Option<usize> = None;
            egui::CollapsingHeader::new("📜 历史").default_open(false).show(ui, |ui| {
                if self.history.is_empty() {
                    ui.label(egui::RichText::new("暂无历史").color(egui::Color32::from_rgb(0xa6,0xad,0xc8)).small());
                } else {
                    egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        for (i, h) in self.history.iter().enumerate() {
                            let label = format!("{} ({}) [{}·{}]", h.title, h.year, h.source, h.format);
                            if secondary_btn(ui, &label).clicked() { clicked_hist = Some(i); }
                        }
                    });
                }
            });
            if let Some(i) = clicked_hist {
                let h = self.history[i].clone();
                let bbcode = if h.format == "bbcode" { h.content.clone() } else { String::new() };
                let json = if h.format == "json" { h.content.clone() } else { String::new() };
                self.right = RightState::Result { bbcode, json };
                self.format = h.format;
            }
        });

        // 底部状态栏
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("源={} 格式={} 类型={}", self.source, self.format, self.media_type))
                    .color(egui::Color32::from_rgb(0xa6,0xad,0xc8)).small());
                ui.separator();
                ui.label(egui::RichText::new(&self.usage_hint).color(egui::Color32::from_rgb(0xa6,0xad,0xc8)).small());
                if !self.status_toast.is_empty() {
                    ui.separator();
                    ui.label(egui::RichText::new(&self.status_toast).color(egui::Color32::from_rgb(0xa6,0xe3,0xa1)).small());
                }
            });
        });
    }
}

fn poster_tile(ui: &mut egui::Ui, c: &Candidate, cell_w: f32, clicked: &std::cell::Cell<Option<usize>>, idx: usize, selected: bool) {
    let amber = egui::Color32::from_rgb(0xF5, 0xA6, 0x23);
    let surface1 = egui::Color32::from_rgb(0x45, 0x47, 0x5a);
    ui.vertical(|ui| {
        ui.set_max_width(cell_w);
        let h = cell_w * 1.5;
        let resp = ui.add(
            egui::Image::new(c.poster.as_str())
                .fit_to_exact_size(egui::vec2(cell_w, h))
                .maintain_aspect_ratio(true)
                .sense(egui::Sense::click()),
        );
        if resp.clicked() { clicked.set(Some(idx)); }
        if resp.hovered() {
            ui.painter().rect_stroke(resp.rect, 4.0_f32, egui::Stroke::new(1.0_f32, surface1));
        }
        if selected {
            ui.painter().rect_stroke(resp.rect.shrink2(egui::vec2(-2.0_f32, -2.0_f32)), 4.0_f32, egui::Stroke::new(2.0_f32, amber));
        }
        // hover tooltip 显示完整信息
        resp.on_hover_ui(|ui| {
            ui.label(egui::RichText::new(if !c.chinese_title.is_empty() { &c.chinese_title } else { &c.title }).strong());
            ui.label(format!("年份: {}", c.year));
            ui.label(format!("评分: {}", c.rating));
            if !c.genre.is_empty() { ui.label(format!("类型: {}", c.genre.join(" / "))); }
            if !c.region.is_empty() { ui.label(format!("地区: {}", c.region)); }
            ui.label(format!("来源: {}", c.source));
        });
        let title = if !c.chinese_title.is_empty() { &c.chinese_title } else { &c.title };
        ui.add(egui::Label::new(egui::RichText::new(title).size(11.0).strong()).wrap_mode(egui::TextWrapMode::Wrap));
        ui.label(egui::RichText::new(format!("{} · {}", c.year, c.rating)).size(10.0).color(egui::Color32::from_rgb(0xa6,0xad,0xc8)));
        if !c.genre.is_empty() {
            ui.label(egui::RichText::new(c.genre.join("/")).size(10.0).color(egui::Color32::from_rgb(0xa6,0xad,0xc8)));
        }
    });
}

// ───────── 输入识别 + 调度（保留）─────────

struct Info {
    kind: String,
    id: String,
    media_type: String,
    query: String,
    year: Option<String>,
}

fn detect(text: &str, force_type: &str) -> Info {
    let t = text.trim();
    if let Some((mt, id)) = regex_simple(t) {
        if t.contains("themoviedb.org/") {
            return Info { kind: "tmdb_url".into(), id, media_type: mt, query: String::new(), year: None };
        }
        return Info { kind: "douban_id".into(), id: mt, media_type: String::new(), query: String::new(), year: None };
    }
    if t.starts_with("tt") && t[2..].chars().all(|c| c.is_ascii_digit()) && t.len() >= 3 {
        return Info { kind: "imdb".into(), id: t.to_string(), media_type: String::new(), query: String::new(), year: None };
    }
    if t.chars().all(|c| c.is_ascii_digit()) {
        if force_type == "movie" || force_type == "tv" {
            return Info { kind: "tmdb_id".into(), id: t.to_string(), media_type: force_type.to_string(), query: String::new(), year: None };
        }
        return Info { kind: "douban_id".into(), id: t.to_string(), media_type: String::new(), query: String::new(), year: None };
    }
    let (query, year) = split_query_year(t);
    Info { kind: "search".into(), id: String::new(), media_type: String::new(), query, year }
}

fn split_query_year(q: &str) -> (String, Option<String>) {
    if let Some(pos) = q.rfind(' ') {
        let y = &q[pos+1..];
        if y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()) {
            return (q[..pos].to_string(), Some(y.to_string()));
        }
    }
    (q.to_string(), None)
}

fn regex_simple(text: &str) -> Option<(String, String)> {
    if let Some(p) = text.find("themoviedb.org/") {
        let rest = &text[p + 15..];
        for mt in ["movie", "tv"] {
            if rest.starts_with(mt) && rest[mt.len()..].starts_with('/') {
                let after = &rest[mt.len() + 1..];
                let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                if !digits.is_empty() { return Some((mt.to_string(), digits)); }
            }
        }
    }
    if let Some(p) = text.find("movie.douban.com/subject/") {
        let rest = &text[p + 25..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() { return Some((digits, String::new())); }
    }
    None
}

enum Outcome {
    Cands(Vec<Candidate>),
    Data(Value),
}

fn phase1(cfg: &Config, input: &str, source: &str, media_type: &str, fc: bool) -> Result<Outcome, String> {
    let info = detect(input, media_type);
    let lim = &cfg.limits;
    match source {
        "douban" => {
            let mut db = doubaninfo::DoubanInfoClient::new(&cfg.keys.doubaninfo, lim.timeout, lim.retry as usize, lim.min_interval);
            match info.kind.as_str() {
                "douban_id" | "imdb" => Ok(Outcome::Data(db.detail(&info.id, info.kind == "imdb")?)),
                "search" => Ok(Outcome::Cands(db.search(&info.query)?)),
                _ => Err(format!("douban 不支持输入: {}", info.kind)),
            }
        }
        "tmdb" => {
            let tm = tmdb::TMDBClient::new(&cfg.keys.tmdb, lim.timeout, lim.retry as usize);
            match info.kind.as_str() {
                "tmdb_url" | "tmdb_id" => Ok(Outcome::Data(tm.detail(info.id.parse().map_err(|e: std::num::ParseIntError| e.to_string())?, &info.media_type, fc)?)),
                "imdb" => {
                    let (mt, tid) = tm.find_by_imdb(&info.id).ok_or("IMDb 未找到")?;
                    Ok(Outcome::Data(tm.detail(tid, &mt, fc)?))
                }
                "search" => Ok(Outcome::Cands(tm.search(&info.query, media_type, info.year.as_deref()))),
                _ => Err(format!("tmdb 不支持输入: {}", info.kind)),
            }
        }
        "auto" => {
            let mut db = doubaninfo::DoubanInfoClient::new(&cfg.keys.doubaninfo, lim.timeout, lim.retry as usize, lim.min_interval);
            let tm = tmdb::TMDBClient::new(&cfg.keys.tmdb, lim.timeout, lim.retry as usize);
            match info.kind.as_str() {
                "douban_id" | "imdb" => Ok(Outcome::Data(db.detail(&info.id, info.kind == "imdb")?)),
                "tmdb_url" | "tmdb_id" => Ok(Outcome::Data(tm.detail(info.id.parse().map_err(|e: std::num::ParseIntError| e.to_string())?, &info.media_type, fc)?)),
                "search" => {
                    let cands = db.search(&info.query)?;
                    if !cands.is_empty() { Ok(Outcome::Cands(cands)) }
                    else if cfg.search.fallback_to_tmdb { Ok(Outcome::Cands(tm.search(&info.query, media_type, info.year.as_deref()))) }
                    else { Ok(Outcome::Cands(vec![])) }
                }
                _ => Err(format!("auto 不支持输入: {}", info.kind)),
            }
        }
        "merge" => {
            let mut db = doubaninfo::DoubanInfoClient::new(&cfg.keys.doubaninfo, lim.timeout, lim.retry as usize, lim.min_interval);
            let tm = tmdb::TMDBClient::new(&cfg.keys.tmdb, lim.timeout, lim.retry as usize);
            let merge_cfg = serde_json::to_value(&cfg.merge).unwrap();
            match info.kind.as_str() {
                "douban_id" | "imdb" => {
                    let d = db.detail(&info.id, info.kind == "imdb")?;
                    let t = tmdb_from_douban(&tm, &d, fc);
                    Ok(Outcome::Data(schema::merge(&d, t.as_ref(), &merge_cfg)))
                }
                "tmdb_url" | "tmdb_id" => {
                    let t = tm.detail(info.id.parse().map_err(|e: std::num::ParseIntError| e.to_string())?, &info.media_type, fc)?;
                    let d = douban_from_tmdb(&mut db, &t);
                    Ok(Outcome::Data(schema::merge(&d, Some(&t), &merge_cfg)))
                }
                "search" => Ok(Outcome::Cands(db.search(&info.query)?)),
                _ => Err(format!("merge 不支持输入: {}", info.kind)),
            }
        }
        _ => Err("未知 source".into()),
    }
}

fn phase2(cfg: &Config, cand: &Candidate, source: &str, fc: bool) -> Result<Value, String> {
    let lim = &cfg.limits;
    match cand.source.as_str() {
        "douban" => {
            let mut db = doubaninfo::DoubanInfoClient::new(&cfg.keys.doubaninfo, lim.timeout, lim.retry as usize, lim.min_interval);
            let d = db.detail(&cand.id, false)?;
            if source == "merge" {
                let tm = tmdb::TMDBClient::new(&cfg.keys.tmdb, lim.timeout, lim.retry as usize);
                let t = tmdb_from_douban(&tm, &d, fc);
                let merge_cfg = serde_json::to_value(&cfg.merge).unwrap();
                Ok(schema::merge(&d, t.as_ref(), &merge_cfg))
            } else {
                Ok(d)
            }
        }
        "tmdb" => {
            let tm = tmdb::TMDBClient::new(&cfg.keys.tmdb, lim.timeout, lim.retry as usize);
            Ok(tm.detail(cand.id.parse().map_err(|e: std::num::ParseIntError| e.to_string())?, &cand.media_type, fc)?)
        }
        _ => Err("未知候选来源".into()),
    }
}

fn tmdb_from_douban(tm: &tmdb::TMDBClient, d: &Value, fc: bool) -> Option<Value> {
    let imdb_id = d.get("imdb_id").and_then(|v| v.as_str()).unwrap_or("");
    if !imdb_id.is_empty() {
        if let Some((mt, tid)) = tm.find_by_imdb(imdb_id) {
            if let Ok(t) = tm.detail(tid, &mt, fc) { return Some(t); }
        }
    }
    let title = d.get("chinese_title").and_then(|v| v.as_str())
        .or_else(|| d.get("original_title").and_then(|v| v.as_str()))
        .or_else(|| d.get("title").and_then(|v| v.as_str())).unwrap_or("");
    let year = d.get("year").and_then(|v| v.as_str()).unwrap_or("");
    let q = if year.is_empty() { title.to_string() } else { format!("{} {}", title, year) };
    let cands = tm.search(&q, "auto", if year.is_empty() { None } else { Some(year) });
    if let Some(c) = cands.first() {
        if let Ok(t) = tm.detail(c.id.parse().unwrap_or(0), &c.media_type, fc) { return Some(t); }
    }
    None
}

fn douban_from_tmdb(db: &mut doubaninfo::DoubanInfoClient, t: &Value) -> Value {
    let imdb_id = t.get("imdb_id").and_then(|v| v.as_str()).unwrap_or("");
    if !imdb_id.is_empty() {
        if let Ok(d) = db.detail(imdb_id, true) { return d; }
    }
    let title = t.get("chinese_title").and_then(|v| v.as_str())
        .or_else(|| t.get("original_title").and_then(|v| v.as_str()))
        .or_else(|| t.get("title").and_then(|v| v.as_str())).unwrap_or("");
    if let Ok(cands) = db.search(title) {
        if let Some(c) = cands.first() {
            if let Ok(d) = db.detail(&c.id, false) { return d; }
        }
    }
    serde_json::json!({})
}

// ───────── 主题/字体 ─────────

fn set_mocha_theme(ctx: &egui::Context) {
    let base = egui::Color32::from_rgb(0x1e, 0x1e, 0x2e);
    let surface0 = egui::Color32::from_rgb(0x31, 0x32, 0x44);
    let surface1 = egui::Color32::from_rgb(0x45, 0x47, 0x5a);
    let surface2 = egui::Color32::from_rgb(0x58, 0x5b, 0x70);
    let overlay0 = egui::Color32::from_rgb(0x6c, 0x70, 0x86);
    let text = egui::Color32::from_rgb(0xcd, 0xd6, 0xf4);
    let subtext = egui::Color32::from_rgb(0xa6, 0xad, 0xc8);
    let amber = egui::Color32::from_rgb(0xF5, 0xA6, 0x23);
    let red = egui::Color32::from_rgb(0xf3, 0x8b, 0xa8);
    let yellow = egui::Color32::from_rgb(0xf9, 0xe2, 0xaf);

    let mut v = egui::Visuals::dark();
    v.panel_fill = base;
    v.window_fill = base;
    v.extreme_bg_color = base;
    v.faint_bg_color = surface0;
    v.widgets.noninteractive.bg_fill = surface0;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, subtext);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, overlay0);
    v.widgets.inactive.bg_fill = surface1;
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, text);
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, overlay0);
    v.widgets.hovered.bg_fill = surface2;
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, text);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, amber);
    v.widgets.active.bg_fill = surface2;
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, text);
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, amber);
    v.widgets.open.bg_fill = surface1;
    v.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, text);
    v.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, amber);
    v.window_stroke = egui::Stroke::new(1.0_f32, overlay0);
    v.selection.bg_fill = amber;
    v.selection.stroke = egui::Stroke::new(1.0_f32, text);
    v.hyperlink_color = amber;
    v.warn_fg_color = yellow;
    v.error_fg_color = red;
    ctx.set_visuals(v);

    let mut s = egui::Style::default();
    s.spacing.item_spacing = egui::vec2(8.0, 8.0);
    s.spacing.button_padding = egui::vec2(10.0, 4.0);
    s.spacing.window_margin = egui::Margin::same(12.0);
    s.spacing.interact_size.y = 22.0;
    ctx.set_style(s);
}

fn install_system_font(ctx: &egui::Context) {
    let candidates: &[&str] = if cfg!(target_os = "windows") {
        &[
            r"C:\Windows\Fonts\HarmonyOS_Sans_SC_Regular.ttf",
            r"C:\Windows\Fonts\msyh.ttc",
        ]
    } else if cfg!(target_os = "macos") {
        &["/System/Library/Fonts/PingFang.ttc"]
    } else {
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
        ]
    };
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert("system_cjk".to_owned(), egui::FontData::from_owned(bytes));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().insert(0, "system_cjk".to_owned());
            }
            ctx.set_fonts(fonts);
            return;
        }
    }
    eprintln!("[warn] 未找到系统 CJK 字体");
}

fn find_config_path() -> Option<String> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("config.toml");
            if p.exists() { return p.to_str().map(String::from); }
        }
    }
    if std::path::Path::new("config.toml").exists() { return Some("config.toml".into()); }
    None
}

fn history_path_for(cfg_path: &str) -> String {
    // 与 config.toml 同目录
    let p = std::path::Path::new(cfg_path);
    if let Some(dir) = p.parent() { dir.join("history.json").to_str().map(String::from).unwrap_or_else(|| "history.json".into()) }
    else { "history.json".into() }
}

fn load_history(path: &str) -> Vec<HistoryItem> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<HistoryItem>>(&s).ok())
        .unwrap_or_default()
}

fn save_history(path: &str, items: &[HistoryItem]) -> std::io::Result<()> {
    let s = serde_json::to_string_pretty(items).map_err(std::io::Error::other)?;
    std::fs::write(path, s)
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs().to_string()).unwrap_or_default()
}
