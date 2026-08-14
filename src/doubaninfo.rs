//! DoubanInfo 客户端：搜索、详情、频控。直译自 Python sources/doubaninfo.py。
use crate::common::{http_get_json, Candidate};
use serde_json::Value;
use std::time::{Duration, Instant};

const BASE: &str = "https://doubaninfo.com/api/v1_douban.php";

pub struct DoubanInfoClient {
    key: String,
    timeout: u64,
    retry: usize,
    min_interval: f64,
    last_req: Option<Instant>,
}

impl DoubanInfoClient {
    pub fn new(key: &str, timeout: u64, retry: usize, min_interval: f64) -> Self {
        Self {
            key: key.to_string(),
            timeout,
            retry,
            min_interval,
            last_req: None,
        }
    }

    fn throttle(&mut self) {
        if self.min_interval > 0.0 {
            if let Some(last) = self.last_req {
                let elapsed = last.elapsed().as_secs_f64();
                if elapsed < self.min_interval {
                    std::thread::sleep(Duration::from_secs_f64(self.min_interval - elapsed));
                }
            }
        }
        self.last_req = Some(Instant::now());
    }

    fn build_url(&self, url_value: &str, douban_mode: bool) -> Result<String, String> {
        let mut url = reqwest::Url::parse(BASE).map_err(|e| e.to_string())?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("key", &self.key);
            query.append_pair("url", url_value);
            if douban_mode {
                query.append_pair("douban", "1");
            }
        }
        Ok(url.to_string())
    }

    /// 名称搜索，直接返回统一候选摘要。
    pub fn search(&mut self, query: &str) -> Result<Vec<Candidate>, String> {
        self.throttle();
        let url = self.build_url(query, false)?;
        let raw = http_get_json(&url, self.timeout, self.retry)?;
        if raw.get("success").and_then(|v| v.as_bool()) != Some(true) {
            return Ok(vec![]);
        }
        if let Some(results) = raw.get("results").and_then(|v| v.as_array()) {
            return Ok(results.iter().map(summarize_douban).collect());
        }
        if raw.get("sid").is_some() {
            return Ok(vec![summarize_douban(&raw)]);
        }
        Ok(vec![])
    }

    /// 取详情（JSON dict，含服务端 format 字段）
    /// value 可为豆瓣 ID/链接或 IMDb ID（IMDb ID 建议 douban_mode=true 取豆瓣信息）
    pub fn detail(&mut self, value: &str, douban_mode: bool) -> Result<Value, String> {
        self.throttle();
        let url = self.build_url(value, douban_mode)?;
        let mut data = http_get_json(&url, self.timeout, self.retry)?;
        // 统一字段：服务端无 source/media_type/douban_link，补齐
        if data.get("success").and_then(|v| v.as_bool()) == Some(true) {
            // 先不可变借用取值，再 as_object_mut 可变借用
            let sc = data
                .get("season_count")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let sid = data
                .get("sid")
                .and_then(|v| v.as_str())
                .map(String::from);
            if let Some(obj) = data.as_object_mut() {
                obj.entry("source").or_insert(Value::String("douban".into()));
                if !obj.contains_key("media_type") {
                    let mt = if !sc.is_empty() { "tv" } else { "movie" };
                    obj.insert("media_type".into(), Value::String(mt.into()));
                }
                if let Some(sid) = sid {
                    obj.entry("douban_link").or_insert(Value::String(format!(
                        "https://movie.douban.com/subject/{}/",
                        sid
                    )));
                }
            }
        }
        Ok(data)
    }
}

fn summarize_douban(r: &Value) -> Candidate {
    let rating = r
        .get("douban_rating")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| r.get("imdb_rating").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    // TV 判定：season_count 非空/非 0 则为剧集
    let sc = r.get("season_count").and_then(|v| v.as_str()).unwrap_or("");
    let media_type = if !sc.is_empty() && sc != "0" { "tv" } else { "movie" };
    Candidate {
        source: "douban".into(),
        id: r.get("sid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        media_type: media_type.into(),
        title: r.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        chinese_title: r.get("chinese_title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        year: r.get("year").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        rating,
        region: r.get("region").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        genre: r
            .get("genre")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        poster: r.get("poster").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    }
}

