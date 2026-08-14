//! TMDB 客户端：搜索、详情（movie/tv），返回统一 schema Value。
//! 直译自 Python sources/tmdb.py。
use crate::common::{
    join_name, parse_release_dates, pick_aka, tmdb_get, Candidate, country_en_to_cn, IMG, ZH,
};
use serde_json::{json, Value};

pub struct TMDBClient {
    key: String,
    timeout: u64,
    retry: usize,
}

impl TMDBClient {
    pub fn new(key: &str, timeout: u64, retry: usize) -> Self {
        Self {
            key: key.to_string(),
            timeout,
            retry,
        }
    }

    fn get(&self, path: &str, params: &[(&str, &str)]) -> Result<Value, String> {
        tmdb_get(path, params, &self.key, self.timeout, self.retry)
    }

    /// 名称搜索，返回候选摘要列表
    pub fn search(&self, query: &str, media_type: &str, year: Option<&str>) -> Vec<Candidate> {
        let q = query.trim();
        let mut results: Vec<(String, Value)> = Vec::new();
        if media_type == "auto" || media_type == "movie" {
            let mut p = vec![("query", q), ("language", ZH), ("page", "1")];
            if let Some(y) = year {
                p.push(("year", y));
            }
            if let Ok(res) = self.get("/search/movie", &p) {
                if let Some(arr) = res.get("results").and_then(|v| v.as_array()) {
                    for x in arr {
                        results.push(("movie".into(), x.clone()));
                    }
                }
            }
        }
        if media_type == "auto" || media_type == "tv" {
            let mut p = vec![("query", q), ("language", ZH), ("page", "1")];
            if let Some(y) = year {
                p.push(("first_air_date_year", y));
            }
            if let Ok(res) = self.get("/search/tv", &p) {
                if let Some(arr) = res.get("results").and_then(|v| v.as_array()) {
                    for x in arr {
                        results.push(("tv".into(), x.clone()));
                    }
                }
            }
        }
        if results.is_empty() {
            return vec![];
        }
        // 年份兜底过滤
        if let Some(y) = year {
            let matched: Vec<(String, Value)> = results
                .iter()
                .filter(|(_, x)| item_year(x).starts_with(y))
                .cloned()
                .collect();
            if !matched.is_empty() {
                results = matched;
            }
        }
        results
            .iter()
            .map(|(mt, x)| summarize_tmdb(mt, x))
            .collect()
    }

    /// 用 IMDb ID 定位，返回 (media_type, tmdb_id)
    pub fn find_by_imdb(&self, imdb_id: &str) -> Option<(String, i64)> {
        let path = format!("/find/{}", imdb_id);
        let d = self.get(&path, &[("external_source", "imdb_id")]).ok()?;
        if let Some(arr) = d.get("movie_results").and_then(|v| v.as_array()) {
            if let Some(id) = arr.first().and_then(|x| x.get("id")).and_then(|v| v.as_u64()) {
                return Some(("movie".into(), id as i64));
            }
        }
        if let Some(arr) = d.get("tv_results").and_then(|v| v.as_array()) {
            if let Some(id) = arr.first().and_then(|x| x.get("id")).and_then(|v| v.as_u64()) {
                return Some(("tv".into(), id as i64));
            }
        }
        None
    }

    /// 详情（movie/tv）
    pub fn detail(&self, tmdb_id: i64, media_type: &str, full_celebrities: bool) -> Result<Value, String> {
        if media_type == "movie" {
            self.get_movie(tmdb_id, full_celebrities)
        } else {
            self.get_tv(tmdb_id, full_celebrities)
        }
    }

    fn get_movie(&self, tmdb_id: i64, full_celebrities: bool) -> Result<Value, String> {
        let app = "credits,alternative_titles,external_ids,translations,release_dates,images";
        let m = self.get(
            &format!("/movie/{}", tmdb_id),
            &[("language", ZH), ("append_to_response", app)],
        )?;
        let credits = m.get("credits").cloned().unwrap_or(json!({}));
        let crew = credits.get("crew").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let cast = credits.get("cast").and_then(|v| v.as_array()).cloned().unwrap_or_default();

        let directors: Vec<String> = crew.iter()
            .filter(|c| c.get("job").and_then(|v| v.as_str()) == Some("Director"))
            .map(|c| join_name(c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                       c.get("original_name").and_then(|v| v.as_str()).unwrap_or("")))
            .collect();
        let writers: Vec<String> = crew.iter()
            .filter(|c| c.get("department").and_then(|v| v.as_str()) == Some("Writing"))
            .map(|c| join_name(c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                       c.get("original_name").and_then(|v| v.as_str()).unwrap_or("")))
            .collect();
        let cast_list: Vec<String> = cast.iter().take(25).map(|c| {
            let name = join_name(c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                               c.get("original_name").and_then(|v| v.as_str()).unwrap_or(""));
            let char = c.get("character").and_then(|v| v.as_str()).unwrap_or("");
            if char.is_empty() { name } else { format!("{} (饰 {})", name, char) }
        }).collect();

        let rel = m.get("release_date").and_then(|v| v.as_str()).unwrap_or("");
        let release_date = parse_release_dates(
            m.get("release_dates").and_then(|v| v.get("results")).unwrap_or(&json!([])),
        );
        let release_date = if !release_date.is_empty() { release_date } else { rel.to_string() };

        let poster = m.get("poster_path").and_then(|v| v.as_str())
            .map(|p| format!("{}{}", IMG, p)).unwrap_or_default();
        let runtime = m.get("runtime").and_then(|v| v.as_u64()).unwrap_or(0);
        let runtime_str = if runtime > 0 { format!("{}分钟", runtime) } else { String::new() };

        let genres: Vec<String> = m.get("genres").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|g| g.get("name").and_then(|v| v.as_str()).map(String::from)).collect())
            .unwrap_or_default();
        let countries: Vec<String> = m.get("production_countries").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|c| c.get("name").and_then(|v| v.as_str()).map(country_en_to_cn_owned)).collect())
            .unwrap_or_default();
        let langs: Vec<String> = m.get("spoken_languages").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|l| l.get("name").and_then(|v| v.as_str()).map(String::from))
                .filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();
        let aka = match m.get("alternative_titles").and_then(|v| v.get("titles")).and_then(|v| v.as_array()) {
            Some(arr) => pick_aka(arr.as_slice(), 20),
            None => vec![],
        };
        let zh_title = zh_title(m.get("translations").unwrap_or(&json!({})), "title");
        let zh_title = if zh_title.is_empty() {
            m.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string()
        } else { zh_title };
        let original_title = m.get("original_title").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let imdb_id = m.get("external_ids").and_then(|v| v.get("imdb_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let vote_average = m.get("vote_average").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let vote_count = m.get("vote_count").and_then(|v| v.as_u64()).unwrap_or(0);

        let mut data = BaseData {
            sid: tmdb_id.to_string(),
            media_type: "movie",
            zh_title,
            original_title,
            year: rel.get(..4).unwrap_or("").to_string(),
            directors,
            writers,
            cast: cast_list,
            genres,
            countries,
            languages: langs,
            aka,
            release_date,
            runtime: runtime_str,
            summary: m.get("overview").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            poster,
            imdb_id,
            vote_average,
            vote_count,
            tmdb_link: format!("https://www.themoviedb.org/movie/{}", tmdb_id),
        }.into_value();
        if full_celebrities {
            let fc = build_fc(&crew, &cast, self, "movie");
            data.as_object_mut().unwrap().insert("full_celebrities".into(), fc);
        }
        Ok(data)
    }

    fn get_tv(&self, tmdb_id: i64, full_celebrities: bool) -> Result<Value, String> {
        let app = "credits,alternative_titles,external_ids,translations,content_ratings,images";
        let m = self.get(
            &format!("/tv/{}", tmdb_id),
            &[("language", ZH), ("append_to_response", app)],
        )?;
        let credits = m.get("credits").cloned().unwrap_or(json!({}));
        let crew = credits.get("crew").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let cast = credits.get("cast").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let created_by = m.get("created_by").and_then(|v| v.as_array()).cloned().unwrap_or_default();

        let directors: Vec<String> = created_by.iter().map(|c| join_name(
            c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            c.get("original_name").and_then(|v| v.as_str()).unwrap_or(""))).collect();
        let writers: Vec<String> = crew.iter()
            .filter(|c| c.get("department").and_then(|v| v.as_str()) == Some("Writing"))
            .map(|c| join_name(c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                       c.get("original_name").and_then(|v| v.as_str()).unwrap_or("")))
            .collect();
        let cast_list: Vec<String> = cast.iter().take(25).map(|c| {
            let name = join_name(c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                               c.get("original_name").and_then(|v| v.as_str()).unwrap_or(""));
            let char = c.get("character").and_then(|v| v.as_str()).unwrap_or("");
            if char.is_empty() { name } else { format!("{} (饰 {})", name, char) }
        }).collect();

        let first_air = m.get("first_air_date").and_then(|v| v.as_str()).unwrap_or("");
        let poster = m.get("poster_path").and_then(|v| v.as_str()).map(|p| format!("{}{}", IMG, p)).unwrap_or_default();
        let eprt: Vec<u64> = m.get("episode_run_time").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_u64()).collect()).unwrap_or_default();
        let runtime_str = if !eprt.is_empty() {
            format!("{}分钟/集", eprt.iter().sum::<u64>() / eprt.len() as u64)
        } else { String::new() };
        let seasons = m.get("number_of_seasons").and_then(|v| v.as_u64()).unwrap_or(0);
        let episodes = m.get("number_of_episodes").and_then(|v| v.as_u64()).unwrap_or(0);
        let mut season_str = if seasons > 0 { format!("{}季", seasons) } else { String::new() };
        if episodes > 0 {
            season_str = if !season_str.is_empty() { format!("{} / {}集", season_str, episodes) } else { format!("{}集", episodes) };
        }

        let genres: Vec<String> = m.get("genres").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|g| g.get("name").and_then(|v| v.as_str()).map(String::from)).collect())
            .unwrap_or_default();
        let countries: Vec<String> = m.get("production_countries").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|c| c.get("name").and_then(|v| v.as_str()).map(country_en_to_cn_owned)).collect())
            .unwrap_or_default();
        let langs: Vec<String> = m.get("spoken_languages").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|l| l.get("name").and_then(|v| v.as_str()).map(String::from))
                .filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();
        let aka = match m.get("alternative_titles").and_then(|v| v.get("results")).and_then(|v| v.as_array()) {
            Some(arr) => pick_aka(arr.as_slice(), 20),
            None => vec![],
        };
        let zh_title = zh_title(m.get("translations").unwrap_or(&json!({})), "name");
        let zh_title = if zh_title.is_empty() {
            m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string()
        } else { zh_title };
        let original_title = m.get("original_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let imdb_id = m.get("external_ids").and_then(|v| v.get("imdb_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let vote_average = m.get("vote_average").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let vote_count = m.get("vote_count").and_then(|v| v.as_u64()).unwrap_or(0);

        let mut data = BaseData {
            sid: tmdb_id.to_string(),
            media_type: "tv",
            zh_title,
            original_title,
            year: first_air.get(..4).unwrap_or("").to_string(),
            directors,
            writers,
            cast: cast_list,
            genres,
            countries,
            languages: langs,
            aka,
            release_date: first_air.to_string(),
            runtime: runtime_str,
            summary: m.get("overview").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            poster,
            imdb_id,
            vote_average,
            vote_count,
            tmdb_link: format!("https://www.themoviedb.org/tv/{}", tmdb_id),
        }.into_value();
        data.as_object_mut().unwrap().insert("season_info".into(), json!(season_str));
        if full_celebrities {
            let cb: Vec<Value> = created_by.iter().map(|c| json!({
                "name": c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                "original_name": c.get("original_name").and_then(|v| v.as_str()).unwrap_or(""),
                "id": c.get("id").and_then(|v| v.as_u64()),
                "job": "Creator"
            })).collect();
            let fc = build_fc(&cb, &cast, self, "tv");
            data.as_object_mut().unwrap().insert("full_celebrities".into(), fc);
        }
        Ok(data)
    }
}

// ───────── 模块级辅助 ─────────

fn zh_title(translations: &Value, key: &str) -> String {
    let arr = match translations.get("translations").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return String::new(),
    };
    for t in arr {
        if t.get("iso_639_1").and_then(|v| v.as_str()) == Some("zh")
            && t.get("iso_3166_1").and_then(|v| v.as_str()) == Some("CN")
        {
            if let Some(v) = t.get("data").and_then(|d| d.get(key)).and_then(|v| v.as_str()) {
                if !v.is_empty() {
                    return v.to_string();
                }
            }
        }
    }
    String::new()
}

fn item_year(x: &Value) -> String {
    let d = x.get("release_date").and_then(|v| v.as_str())
        .or_else(|| x.get("first_air_date").and_then(|v| v.as_str()))
        .unwrap_or("");
    if d.len() >= 4 { d[..4].to_string() } else { String::new() }
}

fn summarize_tmdb(media_type: &str, x: &Value) -> Candidate {
    let poster = x.get("poster_path").and_then(|v| v.as_str())
        .map(|p| format!("{}{}", IMG, p)).unwrap_or_default();
    let rating = x.get("vote_average").and_then(|v| v.as_f64())
        .map(|v| v.to_string()).filter(|s| s != "0").unwrap_or_default();
    Candidate {
        source: "tmdb".into(),
        id: x.get("id").and_then(|v| v.as_u64()).map(|i| i.to_string()).unwrap_or_default(),
        media_type: media_type.into(),
        title: x.get("title").and_then(|v| v.as_str())
            .or_else(|| x.get("name").and_then(|v| v.as_str()))
            .unwrap_or("").to_string(),
        chinese_title: String::new(),
        year: item_year(x),
        rating,
        region: String::new(),
        genre: vec![],
        poster,
    }
}

fn country_en_to_cn_owned(name: &str) -> String {
    country_en_to_cn(name)
}

struct BaseData {
    sid: String,
    media_type: &'static str,
    zh_title: String,
    original_title: String,
    year: String,
    directors: Vec<String>,
    writers: Vec<String>,
    cast: Vec<String>,
    genres: Vec<String>,
    countries: Vec<String>,
    languages: Vec<String>,
    aka: Vec<String>,
    release_date: String,
    runtime: String,
    summary: String,
    poster: String,
    imdb_id: String,
    vote_average: f64,
    vote_count: u64,
    tmdb_link: String,
}

impl BaseData {
    fn into_value(self) -> Value {
        let title = if self.original_title.is_empty() {
            self.zh_title.clone()
        } else {
            format!("{} {}", self.zh_title, self.original_title)
        };
        let rating = if self.vote_average > 0.0 {
            self.vote_average.to_string()
        } else {
            String::new()
        };
        let votes = if self.vote_count > 0 {
            self.vote_count.to_string()
        } else {
            String::new()
        };
        let cover = self.poster.clone();
        let duration = self.runtime.clone();
        json!({
            "success": true,
            "source": "tmdb",
            "media_type": self.media_type,
            "sid": self.sid,
            "title": title,
            "chinese_title": self.zh_title,
            "original_title": self.original_title,
            "year": self.year,
            "director": self.directors,
            "cast": self.cast,
            "genre": self.genres,
            "imdb_id": self.imdb_id,
            "writer": self.writers,
            "release_date": self.release_date,
            "region": self.countries.join(" / "),
            "language": self.languages.join(" / "),
            "duration": duration,
            "aka": self.aka,
            "runtime": self.runtime,
            "summary": self.summary,
            "poster": self.poster,
            "cover": cover,
            "imdb_rating": rating.clone(),
            "imdb_votes": votes.clone(),
            "tmdb_rating": rating,
            "tmdb_votes": votes,
            "tmdb_link": self.tmdb_link,
            "full_celebrities": {},
            "usage": Value::Null,
        })
    }
}

fn build_fc(crew: &[Value], cast: &[Value], client: &TMDBClient, media_type: &str) -> Value {
    let mut fc = serde_json::Map::new();
    // 导演/创作者
    let d_arr: Vec<Value> = crew.iter().take(5)
        .filter(|c| c.get("job").and_then(|v| v.as_str()) == Some("Director")
                  || c.get("job").and_then(|v| v.as_str()) == Some("Creator"))
        .map(|c| person_entry(c, c.get("id").and_then(|v| v.as_u64()), client, "导演 Director", media_type))
        .collect();
    if !d_arr.is_empty() {
        fc.insert("导演 Director".into(), json!(d_arr));
    }
    // 演员
    let c_arr: Vec<Value> = cast.iter().take(20).map(|c| {
        let char = c.get("character").and_then(|v| v.as_str()).unwrap_or("");
        let role = if char.is_empty() { "演员 Actor".to_string() } else { format!("演员 Actor (饰 {})", char) };
        person_entry(c, c.get("id").and_then(|v| v.as_u64()), client, &role, media_type)
    }).collect();
    if !c_arr.is_empty() {
        fc.insert("演员 Cast".into(), json!(c_arr));
    }
    json!(fc)
}

fn person_entry(c: &Value, pid: Option<u64>, client: &TMDBClient, role: &str, media_type: &str) -> Value {
    let name = join_name(
        c.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        c.get("original_name").and_then(|v| v.as_str()).unwrap_or(""),
    );
    let link = pid.map(|id| format!("https://www.themoviedb.org/person/{}", id)).unwrap_or_default();
    let image = c.get("profile_path").and_then(|v| v.as_str())
        .map(|p| format!("{}{}", IMG, p)).unwrap_or_default();
    let character = c.get("character").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut works: Vec<String> = Vec::new();
    if let Some(id) = pid {
        let path = if media_type == "movie" {
            format!("/person/{}/movie_credits", id)
        } else {
            format!("/person/{}/tv_credits", id)
        };
        if let Ok(p) = client.get(&path, &[("language", ZH)]) {
            if let Some(arr) = p.get("cast").and_then(|v| v.as_array()) {
                for x in arr.iter().take(3) {
                    let w = x.get("title").and_then(|v| v.as_str())
                        .or_else(|| x.get("name").and_then(|v| v.as_str()))
                        .or_else(|| x.get("original_title").and_then(|v| v.as_str()))
                        .or_else(|| x.get("original_name").and_then(|v| v.as_str()))
                        .unwrap_or("");
                    if !w.is_empty() {
                        works.push(w.to_string());
                    }
                }
            }
        }
    }
    json!({
        "name": name,
        "link": link,
        "image": image,
        "role": role,
        "character": character,
        "works": works,
    })
}
