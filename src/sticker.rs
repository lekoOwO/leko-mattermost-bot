use crate::database::Database;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sticker {
    pub name: String,
    pub image_url: String,
    pub category: String,
}

impl Sticker {
    /// 取得圖片 URL 的 hash 前八碼
    pub fn get_url_hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.image_url.hash(&mut hasher);
        let hash = hasher.finish();
        format!("{:08x}", hash as u32)
    }

    /// 取得顯示名稱（[分類] 名字 + hash 前八碼）
    pub fn get_display_name(&self) -> String {
        format!(
            "[{}] {} ({})",
            self.category,
            self.name,
            self.get_url_hash()
        )
    }

    // FTS-based tokenization removed: we use simple LIKE-based substring search instead.
}

#[derive(Debug, Clone)]
pub struct StickerDatabase {
    db: Database,
}

impl StickerDatabase {
    /// 建立新的貼圖資料庫（DB-backed）
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 從 CSV 內容載入貼圖資料
    fn load_csv_content_to_vec(
        &self,
        content: &str,
        category: &str,
        source_name: &str,
    ) -> Result<Vec<Sticker>> {
        let mut reader = csv::Reader::from_reader(content.as_bytes());

        // 取得 header
        let headers = reader
            .headers()
            .with_context(|| format!("無法讀取 CSV header: {}", source_name))?;

        // 找到需要的欄位索引
        let name_idx = headers
            .iter()
            .position(|h| h == "名稱")
            .with_context(|| format!("CSV 檔案中找不到「名稱」欄位: {}", source_name))?;

        // 先尋找「圖片」欄位，找不到再找「圖片網址」，最後找「i.imgur」欄位
        let image_url_idx = headers
            .iter()
            .position(|h| h == "圖片")
            .or_else(|| headers.iter().position(|h| h == "圖片網址"))
            .or_else(|| headers.iter().position(|h| h == "i.imgur"))
            .with_context(|| {
                format!(
                    "CSV 檔案中找不到「圖片」、「圖片網址」或「i.imgur」欄位: {}",
                    source_name
                )
            })?;

        let mut stickers: Vec<Sticker> = Vec::new();

        for result in reader.records() {
            let record =
                result.with_context(|| format!("解析 CSV 記錄時發生錯誤: {}", source_name))?;

            let name = record
                .get(name_idx)
                .map(|s| s.to_string())
                .unwrap_or_default();

            let image_url = record
                .get(image_url_idx)
                .map(|s| s.to_string())
                .unwrap_or_default();

            if !name.is_empty() && !image_url.is_empty() {
                stickers.push(Sticker {
                    name,
                    image_url,
                    category: category.to_string(),
                });
            }
        }

        Ok(stickers)
    }

    /// 從 CSV 檔案載入貼圖資料
    pub fn load_csv(&self, path: &str, category: &str) -> Result<Vec<Sticker>> {
        let content =
            fs::read_to_string(path).with_context(|| format!("無法讀取 CSV 檔案: {}", path))?;
        self.load_csv_content_to_vec(&content, category, path)
    }

    /// 從 JSON 內容載入貼圖資料
    fn load_json_content_to_vec(
        &self,
        content: &str,
        category: &str,
        source_name: &str,
    ) -> Result<Vec<Sticker>> {
        let json_data: HashMap<String, String> = serde_json::from_str(content)
            .with_context(|| format!("解析 JSON 檔案時發生錯誤: {}", source_name))?;

        let mut stickers: Vec<Sticker> = Vec::new();
        for (name, image_url) in json_data {
            stickers.push(Sticker {
                name,
                image_url,
                category: category.to_string(),
            });
        }

        Ok(stickers)
    }

    /// 從 JSON 檔案載入貼圖資料
    pub fn load_json(&self, path: &str, category: &str) -> Result<Vec<Sticker>> {
        let content =
            fs::read_to_string(path).with_context(|| format!("無法讀取 JSON 檔案: {}", path))?;
        self.load_json_content_to_vec(&content, category, path)
    }

    /// 從 HTTP GET 獲取資料並載入
    pub async fn load_from_http(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        format: &crate::config::FileFormat,
        category: &str,
    ) -> Result<Vec<Sticker>> {
        let client = reqwest::Client::new();
        let mut request = client.get(url);

        // 添加自定義 headers
        for (key, value) in headers {
            request = request.header(key, value);
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("無法從 URL 獲取資料: {}", url))?;

        let content = response
            .text()
            .await
            .with_context(|| format!("無法讀取 HTTP 回應內容: {}", url))?;

        match format {
            crate::config::FileFormat::Csv => self.load_csv_content_to_vec(&content, category, url),
            crate::config::FileFormat::Json => {
                self.load_json_content_to_vec(&content, category, url)
            }
        }
    }

    /// 從配置載入所有貼圖資料
    /// Load stickers from config and insert them into the provided Database.
    pub async fn load_from_config(
        db: &Database,
        config: &crate::config::StickersConfig,
    ) -> Result<Self> {
        let loader = Self::new(db.clone());
        let mut all: Vec<Sticker> = Vec::new();

        for category_config in &config.categories {
            for source in &category_config.sources {
                match source {
                    crate::config::SourceConfig::File { format, path } => match format {
                        crate::config::FileFormat::Csv => {
                            let mut v = loader
                                .load_csv_content_to_vec(
                                    &fs::read_to_string(path)?,
                                    &category_config.name,
                                    path,
                                )
                                .with_context(|| format!("載入 CSV 檔案失敗: {}", path))?;
                            all.append(&mut v);
                        }
                        crate::config::FileFormat::Json => {
                            let mut v = loader
                                .load_json(path, &category_config.name)
                                .with_context(|| format!("載入 JSON 檔案失敗: {}", path))?;
                            all.append(&mut v);
                        }
                    },
                    crate::config::SourceConfig::HttpGet {
                        format,
                        url,
                        headers,
                    } => {
                        let mut v = loader
                            .load_from_http(url, headers, format, &category_config.name)
                            .await
                            .with_context(|| format!("從 HTTP 載入資料失敗: {}", url))?;
                        all.append(&mut v);
                    }
                }
            }
        }

        // Replace stickers in DB so the stored state matches the config exactly.
        db.replace_stickers(&all)
            .await
            .with_context(|| "寫入貼圖到資料庫失敗")?;

        Ok(loader)
    }

    /// 取得所有分類
    pub async fn get_categories(&self) -> Result<Vec<String>> {
        let stats = self.db.get_sticker_category_stats().await?;
        let mut categories: Vec<String> = stats.keys().cloned().collect();
        categories.sort();
        Ok(categories)
    }

    /// 取得每個分類的貼圖數量統計
    pub async fn get_category_stats(&self) -> Result<HashMap<String, i64>> {
        self.db.get_sticker_category_stats().await
    }

    /// 取得貼圖總數
    pub async fn get_total_count(&self) -> Result<i64> {
        self.db.count_stickers().await
    }

    /// 取得所有貼圖
    /// Return all stickers (not recommended for very large DBs)
    pub async fn get_all(&self) -> Result<Vec<Sticker>> {
        // Small helper: reuse search with empty criteria and large limit
        Ok(self
            .db
            .search_stickers(None, &[], &[], None, 10_000)
            .await?)
    }

    /// 解析搜尋查詢
    /// 格式：[分類:] 關鍵字1 關鍵字2 -排除詞
    /// 例如：
    /// - "a b" -> 必須包含 a 和 b
    /// - "海綿寶寶: a" -> 在海綿寶寶分類中搜尋 a
    /// - "-123" -> 不包含 123
    /// - "海綿寶寶: a b -c" -> 在海綿寶寶分類中搜尋包含 a 和 b 但不包含 c
    fn parse_query(query: &str) -> (Option<String>, Vec<String>, Vec<String>) {
        let query = query.trim();

        // 檢查是否有分類指定（格式：分類: 關鍵字）
        let (category, keyword_part) = if let Some(colon_pos) = query.find(':') {
            let cat = query[..colon_pos].trim().to_string();
            let kw = query[colon_pos + 1..].trim();
            (Some(cat), kw)
        } else {
            (None, query)
        };

        // 解析關鍵字和排除詞
        let mut include_keywords: Vec<String> = Vec::new();
        let mut exclude_keywords: Vec<String> = Vec::new();

        for token in keyword_part.split_whitespace() {
            if let Some(excluded) = token.strip_prefix('-') {
                if !excluded.is_empty() {
                    exclude_keywords.push(excluded.to_lowercase());
                }
            } else if !token.is_empty() {
                include_keywords.push(token.to_lowercase());
            }
        }

        (category, include_keywords, exclude_keywords)
    }

    /// 根據分類和關鍵字搜尋貼圖
    /// 支援進階搜尋語法：
    /// - 空格分隔多個關鍵字（AND 條件）
    /// - `分類: 關鍵字` 指定分類搜尋
    /// - `-關鍵字` 排除包含該關鍵字的結果
    /// For backward compatibility this returns an empty Vec; use `search_async` instead.
    pub fn search(&self, _keyword: &str, _categories: Option<&[String]>) -> Vec<&Sticker> {
        vec![]
    }

    /// Async search that queries the DB and returns matching stickers
    pub async fn search_async(
        &self,
        keyword: &str,
        categories: Option<&[String]>,
    ) -> Result<Vec<Sticker>> {
        let (query_category, include_keywords, exclude_keywords) = Self::parse_query(keyword);
        let res = self
            .db
            .search_stickers(
                query_category.as_deref(),
                &include_keywords,
                &exclude_keywords,
                categories,
                100,
            )
            .await?;
        Ok(res)
    }

    /// 根據索引取得貼圖
    pub fn get_by_index(&self, _index: usize) -> Option<&Sticker> {
        // Not supported in DB-backed mode
        None
    }

    /// 取得貼圖數量
    pub async fn count(&self) -> Result<i64> {
        self.db.count_stickers().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::utils::setup_db;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_get_url_hash() {
        let sticker = Sticker {
            name: "測試".to_string(),
            image_url: "https://i.imgur.com/XB4MwpR.jpg".to_string(),
            category: "測試分類".to_string(),
        };

        let hash = sticker.get_url_hash();
        assert_eq!(hash.len(), 8);

        let display_name = sticker.get_display_name();
        assert!(display_name.starts_with("[測試分類] 測試 ("));
        assert!(display_name.ends_with(")"));
    }

    #[tokio::test]
    async fn test_load_csv() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("test.csv");

        let csv_content = r#"日期,流水號,名稱,維基集數,ESFIO,無文字版本,imgur,i.imgur
191111,SS0001,你為什麼不問問神奇海螺呢,42-A,S3E03,,https://imgur.com/XB4MwpR,https://i.imgur.com/XB4MwpR.jpg
191111,SS0002,你現在是在懷疑神奇海螺的神奇魔力嗎,42-A,S3E03,,https://imgur.com/Mz19r2y,https://i.imgur.com/Mz19r2y.jpg"#;

        fs::write(&csv_path, csv_content).unwrap();

        // use loader to parse CSV into vec, then insert into DB
        let database = setup_db().await;
        let loader = StickerDatabase::new(database.clone());
        let v = loader
            .load_csv(csv_path.to_str().unwrap(), "測試分類")
            .unwrap();
        assert_eq!(v.len(), 2);
        let inserted = database.bulk_insert_stickers(&v).await.unwrap();
        assert!(inserted >= 2);
        let cnt = database.count_stickers().await.unwrap();
        assert!(cnt >= 2);
    }

    #[tokio::test]
    async fn test_load_csv_with_image_column() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("test.csv");

        // 測試使用「圖片」欄位
        let csv_content = r#"名稱,圖片,其他欄位
測試貼圖1,https://example.com/test1.jpg,test
測試貼圖2,https://example.com/test2.jpg,test"#;

        fs::write(&csv_path, csv_content).unwrap();

        let database = setup_db().await;
        let loader = StickerDatabase::new(database.clone());
        let v = loader.load_csv(csv_path.to_str().unwrap(), "其他").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "測試貼圖1");
        assert_eq!(v[0].image_url, "https://example.com/test1.jpg");
        assert_eq!(v[0].category, "其他");
    }

    #[tokio::test]
    async fn test_load_json() {
        let temp_dir = TempDir::new().unwrap();
        let json_path = temp_dir.path().join("test.json");

        let json_content = r#"{
    "你很廉價": "https://i.imgur.com/gQRSLIx.png",
    "測試貼圖": "https://example.com/test.png"
}"#;

        fs::write(&json_path, json_content).unwrap();

        let database = setup_db().await;
        let loader = StickerDatabase::new(database.clone());
        let v = loader
            .load_json(json_path.to_str().unwrap(), "JSON分類")
            .unwrap();
        assert_eq!(v.len(), 2);
        assert!(v.iter().all(|s| s.category == "JSON分類"));
    }

    #[tokio::test]
    async fn test_search() {
        let database = setup_db().await;
        let stickers = vec![
            Sticker {
                name: "測試海螺".to_string(),
                image_url: "https://example.com/1.jpg".to_string(),
                category: "分類A".to_string(),
            },
            Sticker {
                name: "派大星".to_string(),
                image_url: "https://example.com/2.jpg".to_string(),
                category: "分類B".to_string(),
            },
        ];
        let inserted = database.bulk_insert_stickers(&stickers).await.unwrap();
        assert!(inserted >= 2);

        let results = database
            .search_stickers(None, &vec!["海螺".to_string()], &vec![], None, 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "測試海螺");

        let results = database
            .search_stickers(None, &vec![], &vec![], Some(&["分類A".to_string()]), 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "測試海螺");
    }

    #[tokio::test]
    async fn test_search_advanced() {
        let database = setup_db().await;
        let stickers = vec![
            Sticker {
                name: "開心派大星".to_string(),
                image_url: "https://example.com/1.jpg".to_string(),
                category: "海綿寶寶".to_string(),
            },
            Sticker {
                name: "難過派大星".to_string(),
                image_url: "https://example.com/2.jpg".to_string(),
                category: "海綿寶寶".to_string(),
            },
            Sticker {
                name: "開心章魚哥".to_string(),
                image_url: "https://example.com/3.jpg".to_string(),
                category: "海綿寶寶".to_string(),
            },
            Sticker {
                name: "開心小新".to_string(),
                image_url: "https://example.com/4.jpg".to_string(),
                category: "蠟筆小新".to_string(),
            },
        ];
        database.bulk_insert_stickers(&stickers).await.unwrap();

        // 多關鍵字 AND
        let results = database
            .search_stickers(
                None,
                &vec!["開心".to_string(), "派大星".to_string()],
                &vec![],
                None,
                10,
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "開心派大星");

        // 分類搜尋
        let results = database
            .search_stickers(
                Some("海綿寶寶"),
                &vec!["開心".to_string()],
                &vec![],
                None,
                10,
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|s| s.name.contains("開心")));
        assert!(results.iter().all(|s| s.category == "海綿寶寶"));

        // 排除
        let results = database
            .search_stickers(
                None,
                &vec!["開心".to_string()],
                &vec!["派大星".to_string()],
                None,
                10,
            )
            .await
            .unwrap();
        assert!(results.iter().all(|s| !s.name.contains("派大星")));

        // 分類 + 排除
        let results = database
            .search_stickers(
                Some("海綿寶寶"),
                &vec![],
                &vec!["章魚哥".to_string()],
                None,
                10,
            )
            .await
            .unwrap();
        assert!(results.iter().all(|s| s.category == "海綿寶寶"));
        assert!(results.iter().all(|s| !s.name.contains("章魚哥")));

        // 分類 + 多關鍵字 + 排除
        let results = database
            .search_stickers(
                Some("海綿寶寶"),
                &vec!["派大星".to_string()],
                &vec!["難過".to_string()],
                None,
                10,
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "開心派大星");
    }

    #[tokio::test]
    async fn test_get_categories() {
        let database = setup_db().await;
        let stickers = vec![
            Sticker {
                name: "測試1".to_string(),
                image_url: "https://example.com/1.jpg".to_string(),
                category: "分類A".to_string(),
            },
            Sticker {
                name: "測試2".to_string(),
                image_url: "https://example.com/2.jpg".to_string(),
                category: "分類B".to_string(),
            },
            Sticker {
                name: "測試3".to_string(),
                image_url: "https://example.com/3.jpg".to_string(),
                category: "分類A".to_string(),
            },
        ];
        database.bulk_insert_stickers(&stickers).await.unwrap();

        let categories_map = database.get_sticker_category_stats().await.unwrap();
        let mut categories: Vec<String> = categories_map.keys().cloned().collect();
        categories.sort();
        assert_eq!(categories.len(), 2);
        assert!(categories.contains(&"分類A".to_string()));
        assert!(categories.contains(&"分類B".to_string()));
    }

    #[tokio::test]
    async fn test_load_from_config_replaces_existing() {
        use crate::config::{CategoryConfig, FileFormat, SourceConfig, StickersConfig};

        let database = setup_db().await;

        // Create first JSON file with stickers a and b
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("set1.json");
        let json1 = r#"{"a": "https://example.com/a.png", "b": "https://example.com/b.png"}"#;
        fs::write(&file1, json1).unwrap();

        let cat1 = CategoryConfig {
            name: "CAT1".to_string(),
            sources: vec![SourceConfig::File {
                format: FileFormat::Json,
                path: file1.to_string_lossy().to_string(),
            }],
        };

        let cfg1 = StickersConfig {
            categories: vec![cat1],
        };

        // Load first config
        let _loader1 = StickerDatabase::load_from_config(&database, &cfg1)
            .await
            .expect("load1");

        let cnt1 = database.count_stickers().await.expect("count1");
        assert_eq!(cnt1, 2);

        // Create second JSON file with stickers b and c (a should be removed)
        let file2 = temp_dir.path().join("set2.json");
        let json2 = r#"{"b": "https://example.com/b.png", "c": "https://example.com/c.png"}"#;
        fs::write(&file2, json2).unwrap();

        let cat2 = CategoryConfig {
            name: "CAT1".to_string(),
            sources: vec![SourceConfig::File {
                format: FileFormat::Json,
                path: file2.to_string_lossy().to_string(),
            }],
        };

        let cfg2 = StickersConfig {
            categories: vec![cat2],
        };

        // Load second config (should replace existing stickers)
        let _loader2 = StickerDatabase::load_from_config(&database, &cfg2)
            .await
            .expect("load2");

        let cnt2 = database.count_stickers().await.expect("count2");
        assert_eq!(cnt2, 2);

        // Ensure 'a' is gone and 'c' exists
        let res_a = database
            .search_stickers(None, &vec!["a".to_string()], &vec![], None, 10)
            .await
            .unwrap();
        assert_eq!(res_a.len(), 0);
        let res_c = database
            .search_stickers(None, &vec!["c".to_string()], &vec![], None, 10)
            .await
            .unwrap();
        assert_eq!(res_c.len(), 1);
    }
}
