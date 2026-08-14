//! 公共数据工具：HTTP 客户端、国家映射、字符串工具。
//! 直译自 Python sources/common.py，统一用 serde_json::Value 流转。
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

static HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();

fn http_client() -> &'static reqwest::blocking::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent("doubangen/0.1")
            .build()
            .expect("failed to build HTTP client")
    })
}

pub const IMG: &str = "https://image.tmdb.org/t/p/original";
pub const ZH: &str = "zh-CN";

/// 主要地区代码 → 中文名
pub fn country_cn(code: &str) -> &str {
    match code {
        "US" => "美国", "CN" => "中国大陆", "HK" => "中国香港", "TW" => "中国台湾",
        "JP" => "日本", "KR" => "韩国", "GB" => "英国", "FR" => "法国", "DE" => "德国",
        "CA" => "加拿大", "AU" => "澳大利亚", "IT" => "意大利", "ES" => "西班牙",
        "IN" => "印度", "RU" => "俄罗斯", "TH" => "泰国", "SG" => "新加坡",
        "MX" => "墨西哥", "BR" => "巴西", "NL" => "荷兰", "SE" => "瑞典",
        "IE" => "爱尔兰", "NZ" => "新西兰", "NO" => "挪威", "DK" => "丹麦",
        "FI" => "芬兰", "PL" => "波兰", "TR" => "土耳其", "ZA" => "南非",
        _ => code,
    }
}

/// TMDB production_countries.name（英文）→ 中文
pub fn country_en_to_cn(name: &str) -> String {
    let mapped = match name {
        "United States of America" | "United States" => "美国",
        "China" => "中国大陆",
        "Hong Kong" => "中国香港",
        "Taiwan" => "中国台湾",
        "Japan" => "日本",
        "South Korea" | "Korea" => "韩国",
        "United Kingdom" => "英国",
        "France" => "法国",
        "Germany" => "德国",
        "Canada" => "加拿大",
        "Australia" => "澳大利亚",
        "Italy" => "意大利",
        "Spain" => "西班牙",
        "India" => "印度",
        "Russia" => "俄罗斯",
        _ => return name.to_string(),
    };
    mapped.to_string()
}

/// movie release_dates 优先展示地区
pub const PRIORITY_COUNTRIES: &[&str] = &["CN", "US", "HK", "TW", "JP"];

/// 合并 name 与 original_name，相同则只保留一个
pub fn join_name(name: &str, original: &str) -> String {
    let name = name.trim();
    let original = original.trim();
    if original.is_empty() || original == name {
        return name.to_string();
    }
    format!("{} {}", name, original)
}

/// 去重并截断别名列表。titles 元素可为 {title:...} 对象或字符串。
pub fn pick_aka(titles: &[Value], limit: usize) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut aka = Vec::new();
    for t in titles {
        let title = if t.is_object() {
            t.get("title").and_then(|v| v.as_str()).unwrap_or("")
        } else {
            t.as_str().unwrap_or("")
        };
        if !title.is_empty() && seen.insert(title.to_string()) {
            aka.push(title.to_string());
            if aka.len() >= limit {
                break;
            }
        }
    }
    aka
}

/// movie release_dates.results → 主要国家首映日期字符串
pub fn parse_release_dates(rd: &Value) -> String {
    let results = match rd.as_array() {
        Some(a) => a,
        None => return String::new(),
    };
    let mut picked: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    for r in results {
        let tag = match r.get("iso_3166_1").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };
        if !PRIORITY_COUNTRIES.contains(&tag) {
            continue;
        }
        if let Some(dates) = r.get("release_dates").and_then(|v| v.as_array()) {
            for d in dates {
                let s = d.get("release_date").and_then(|v| v.as_str()).unwrap_or("");
                let s = &s[..s.len().min(10)];
                if s.is_empty() {
                    continue;
                }
                let entry = picked.entry(tag).or_insert_with(|| s.to_string());
                if s < entry.as_str() {
                    *entry = s.to_string();
                }
            }
        }
    }
    PRIORITY_COUNTRIES
        .iter()
        .filter_map(|&tag| picked.get(tag).map(|s| format!("{}({})", s, country_cn(tag))))
        .collect::<Vec<_>>()
        .join(" / ")
}

/// GET 请求并解析 JSON，复用全局连接池并带 429/网络重试。
pub fn http_get_json(url: &str, timeout: u64, retry: usize) -> Result<Value, String> {
    let mut last = String::new();
    for i in 0..=retry {
        match http_client()
            .get(url)
            .timeout(Duration::from_secs(timeout))
            .send()
        {
            Ok(r) => {
                let status = r.status();
                if status.is_success() {
                    return r.json::<Value>().map_err(|e| e.to_string());
                }
                last = format!("HTTP {}", status);
                if status.as_u16() != 429 || i == retry {
                    return Err(last);
                }
            }
            Err(e) => {
                last = e.to_string();
                if i == retry {
                    return Err(last);
                }
            }
        }
        std::thread::sleep(Duration::from_secs(i as u64 + 1));
    }
    Err(last)
}

/// TMDB API 调用：自动附加 api_key
pub fn tmdb_get(
    path: &str,
    params: &[(&str, &str)],
    key: &str,
    timeout: u64,
    retry: usize,
) -> Result<Value, String> {
    let mut url = reqwest::Url::parse(&format!("https://api.themoviedb.org/3{}", path))
        .map_err(|e| e.to_string())?;
    {
        let mut query = url.query_pairs_mut();
        query.extend_pairs(params.iter().copied());
        query.append_pair("api_key", key);
    }
    http_get_json(url.as_str(), timeout, retry)
}

/// 搜索候选摘要（豆瓣/TMDB 统一），供 GUI 海报墙展示
#[derive(Clone)]
pub struct Candidate {
    pub source: String,   // "douban" / "tmdb"
    pub id: String,
    pub media_type: String, // "movie" / "tv"
    pub title: String,
    pub chinese_title: String,
    pub year: String,
    pub rating: String,
    pub region: String,
    pub genre: Vec<String>,
    pub poster: String,
}
