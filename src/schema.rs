//! 统一数据 schema：BBCode 渲染、merge 聚合、空字段清理。
//! 直译自 Python schema.py。
use serde_json::{json, Map, Value};

/// 判断是否电视剧
pub fn is_tv(d: &Value) -> bool {
    if d.get("media_type").and_then(|v| v.as_str()) == Some("tv") {
        return true;
    }
    if d.get("season_info")
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    let sc = d.get("season_count").and_then(|v| v.as_str()).unwrap_or("");
    if !sc.is_empty() {
        return true;
    }
    if d.get("tmdb_link")
        .and_then(|v| v.as_str())
        .map(|s| s.contains("/tv/"))
        .unwrap_or(false)
    {
        return true;
    }
    false
}

/// 一行：◎label 全角对齐到 5 宽 + value
fn row(label: &str, value: &str) -> String {
    let mut s = format!("◎{}", label);
    let pad = 5usize.saturating_sub(label.chars().count());
    for _ in 0..pad {
        s.push('　');
    }
    s.push('　');
    s.push_str(value);
    s
}

/// 取字符串字段
fn s(d: &Value, key: &str) -> String {
    d.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

/// 取数组字段（字符串元素）
fn arr_str(d: &Value, key: &str) -> Vec<String> {
    d.get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// 统一 BBCode 渲染，支持豆瓣 + TMDB 全字段，空字段自动跳过
pub fn to_bbcode(d: &Value) -> String {
    let tv = is_tv(d);
    let mut lines: Vec<String> = Vec::new();

    let poster = s(d, "poster");
    if !poster.is_empty() {
        lines.push(format!("[img]{}[/img]", poster));
        lines.push(String::new());
    }

    let name_label = if tv { "剧　名" } else { "片　名" };
    let names: Vec<String> = [s(d, "chinese_title"), s(d, "original_title")]
        .iter()
        .filter(|x| !x.is_empty())
        .cloned()
        .collect();
    if !names.is_empty() {
        lines.push(row(name_label, &names.join(" / ")));
    }

    let aka = arr_str(d, "aka");
    if !aka.is_empty() {
        lines.push(row("译　名", &aka.join(" / ")));
    }
    let year = s(d, "year");
    if !year.is_empty() {
        lines.push(row("年　代", &year));
    }
    let region = s(d, "region");
    if !region.is_empty() {
        lines.push(row("产　地", &region));
    }
    let genre = arr_str(d, "genre");
    if !genre.is_empty() {
        lines.push(row("类　别", &genre.join(" / ")));
    }
    let language = s(d, "language");
    if !language.is_empty() {
        lines.push(row("语　言", &language));
    }
    let release_date = s(d, "release_date");
    if !release_date.is_empty() {
        let date_label = if tv { "首播日期" } else { "上映日期" };
        lines.push(row(date_label, &release_date));
    }
    let season_info = s(d, "season_info");
    if !season_info.is_empty() {
        lines.push(row("季　数", &season_info));
    }

    // 评分行
    let imdb_rating = s(d, "imdb_rating");
    let imdb_votes = s(d, "imdb_votes");
    if !imdb_rating.is_empty() {
        lines.push(row("IMDb评分", &format!("{} /10 ({} 人评价)", imdb_rating, imdb_votes)));
    }
    let douban_rating = s(d, "douban_rating");
    let douban_votes = s(d, "douban_votes");
    if !douban_rating.is_empty() {
        lines.push(row("豆瓣评分", &format!("{} /10 ({} 人评价)", douban_rating, douban_votes)));
    }
    let tmdb_rating = s(d, "tmdb_rating");
    let tmdb_votes = s(d, "tmdb_votes");
    if !tmdb_rating.is_empty() {
        lines.push(row("TMDB评分", &format!("{} /10 ({} 人评价)", tmdb_rating, tmdb_votes)));
    }

    // 链接
    let imdb_id = s(d, "imdb_id");
    if !imdb_id.is_empty() {
        lines.push(row("IMDb链接", &format!("https://www.imdb.com/title/{}/", imdb_id)));
    }
    let douban_link = s(d, "douban_link");
    if !douban_link.is_empty() {
        lines.push(row("豆瓣链接", &douban_link));
    }
    let tmdb_link = s(d, "tmdb_link");
    if !tmdb_link.is_empty() {
        lines.push(row("TMDB链接", &tmdb_link));
    }

    let runtime = s(d, "runtime");
    if !runtime.is_empty() {
        let len_label = if tv { "每集时长" } else { "片　长" };
        lines.push(row(len_label, &runtime));
    }

    let director = arr_str(d, "director");
    if !director.is_empty() {
        let dir_label = if tv { "创作者" } else { "导　演" };
        lines.push(row(dir_label, &director.join(" / ")));
    }
    let writer = arr_str(d, "writer");
    if !writer.is_empty() {
        lines.push(row("编　剧", &writer.join(" / ")));
    }
    let cast = arr_str(d, "cast");
    if !cast.is_empty() {
        lines.push(row("主　演", &cast[0]));
        for c in &cast[1..] {
            lines.push(format!("　　　　　　{}", c));
        }
    }

    let summary = s(d, "summary");
    if !summary.is_empty() {
        lines.push(String::new());
        let sum_label = if tv { "剧情简介" } else { "简　介" };
        lines.push(row(sum_label, ""));
        lines.push(format!("　　{}", summary));
    }

    // 获奖
    if let Some(awards) = d.get("awards").and_then(|v| v.as_array()) {
        if !awards.is_empty() {
            lines.push(String::new());
            lines.push(row("获奖情况", ""));
            for a in awards {
                lines.push(String::new());
                let festival = a.get("festival").and_then(|v| v.as_str()).unwrap_or("");
                lines.push(format!("　　{}", festival));
                if let Some(ws) = a.get("awards").and_then(|v| v.as_array()) {
                    for w in ws {
                        if let Some(t) = w.as_str() {
                            lines.push(format!("　　{}", t));
                        }
                    }
                }
            }
        }
    }

    lines.join("\n")
}

/// 豆瓣优先聚合：豆瓣主体 + TMDB 补充
pub fn merge(douban: &Value, tmdb: Option<&Value>, merge_cfg: &serde_json::Value) -> Value {
    let mut base = douban.clone();
    set_str(&mut base, "source", "merge");

    if let Some(tmdb) = tmdb {
        if let Some(mt) = tmdb.get("media_type").and_then(|v| v.as_str()) {
            set_str(&mut base, "media_type", mt);
        }
        if merge_cfg.get("enrich_tmdb_rating").and_then(|v| v.as_bool()).unwrap_or(true) {
            if let Some(r) = tmdb.get("tmdb_rating") {
                base.as_object_mut().unwrap().insert("tmdb_rating".into(), r.clone());
            }
            for (k, src_k) in [("tmdb_votes", "tmdb_votes"), ("tmdb_link", "tmdb_link")] {
                if let Some(v) = tmdb.get(src_k) {
                    base.as_object_mut().unwrap().insert(k.into(), v.clone());
                }
            }
        }
        if merge_cfg.get("enrich_tmdb_aka").and_then(|v| v.as_bool()).unwrap_or(true) {
            let mut aka = arr_str(&base, "aka");
            for x in arr_str(tmdb, "aka") {
                if !aka.contains(&x) {
                    aka.push(x);
                }
            }
            base.as_object_mut().unwrap().insert("aka".into(), json!(aka));
        }
        if merge_cfg.get("enrich_celebrities").and_then(|v| v.as_bool()).unwrap_or(true) {
            let db_fc = base.get("full_celebrities").cloned().unwrap_or(json!({}));
            let tmdb_fc = tmdb.get("full_celebrities").cloned().unwrap_or(json!({}));
            let merged = merge_fc(&db_fc, &tmdb_fc);
            base.as_object_mut().unwrap().insert("full_celebrities".into(), merged);
        }
        // original_title / imdb_id 兜底
        if s(&base, "original_title").is_empty() {
            let ot = s(tmdb, "original_title");
            if !ot.is_empty() {
                set_str(&mut base, "original_title", &ot);
            }
        }
    }

    // 重新渲染 format
    let bbcode = to_bbcode(&base);
    base.as_object_mut().unwrap().insert("format".into(), json!(bbcode));
    base
}

fn set_str(d: &mut Value, key: &str, val: &str) {
    if let Some(obj) = d.as_object_mut() {
        obj.insert(key.into(), json!(val));
    }
}

fn merge_fc(db: &Value, tmdb: &Value) -> Value {
    if tmdb.is_null() { return db.clone(); }
    if db.is_null() { return tmdb.clone(); }
    let mut merged = Map::new();
    let db_map = db.as_object().cloned().unwrap_or_default();
    let tmdb_map = tmdb.as_object().cloned().unwrap_or_default();
    let mut keys: Vec<String> = db_map.keys().cloned().collect();
    for k in tmdb_map.keys() {
        if !keys.contains(k) {
            keys.push(k.clone());
        }
    }
    for k in keys {
        let db_list = db_map.get(&k).and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let tmdb_list = tmdb_map.get(&k).and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut out: Vec<Value> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for item in db_list.iter().chain(tmdb_list.iter()) {
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !name.is_empty() && seen.contains(&name) {
                continue;
            }
            if !name.is_empty() {
                seen.push(name.clone());
            }
            out.push(item.clone());
        }
        merged.insert(k, json!(out));
    }
    json!(merged)
}

/// 递归过滤空字段（"" / [] / {} / Null），用于 JSON 输出干净
pub fn clean_output(data: Value) -> Value {
    if is_empty(&data) {
        return Value::Null;
    }
    match data {
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, v) in m {
                let cleaned = clean_output(v);
                if !is_empty(&cleaned) {
                    out.insert(k, cleaned);
                }
            }
            Value::Object(out)
        }
        Value::Array(a) => {
            let out: Vec<Value> = a.into_iter().map(clean_output).filter(|v| !is_empty(v)).collect();
            Value::Array(out)
        }
        other => other,
    }
}

fn is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(m) => m.is_empty(),
        _ => false,
    }
}
