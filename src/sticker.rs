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
}

#[derive(Debug)]
pub struct StickerDatabase {
    stickers: Vec<Sticker>,
}

impl StickerDatabase {
    /// 建立新的貼圖資料庫
    pub fn new() -> Self {
        Self {
            stickers: Vec::new(),
        }
    }

    /// 從 CSV 檔案載入貼圖資料
    pub fn load_csv(&mut self, path: &str, category: &str) -> Result<()> {
        let content =
            fs::read_to_string(path).with_context(|| format!("無法讀取 CSV 檔案: {}", path))?;

        let mut reader = csv::Reader::from_reader(content.as_bytes());

        // 取得 header
        let headers = reader
            .headers()
            .with_context(|| format!("無法讀取 CSV header: {}", path))?;

        // 找到需要的欄位索引
        let name_idx = headers
            .iter()
            .position(|h| h == "名稱")
            .with_context(|| format!("CSV 檔案中找不到「名稱」欄位: {}", path))?;

        // 先尋找「圖片」欄位，找不到再找「圖片網址」，最後找「i.imgur」欄位
        let image_url_idx = headers
            .iter()
            .position(|h| h == "圖片")
            .or_else(|| headers.iter().position(|h| h == "圖片網址"))
            .or_else(|| headers.iter().position(|h| h == "i.imgur"))
            .with_context(|| {
                format!(
                    "CSV 檔案中找不到「圖片」、「圖片網址」或「i.imgur」欄位: {}",
                    path
                )
            })?;

        for result in reader.records() {
            let record = result.with_context(|| format!("解析 CSV 記錄時發生錯誤: {}", path))?;

            let name = record
                .get(name_idx)
                .map(|s| s.to_string())
                .unwrap_or_default();

            let image_url = record
                .get(image_url_idx)
                .map(|s| s.to_string())
                .unwrap_or_default();

            if !name.is_empty() && !image_url.is_empty() {
                self.stickers.push(Sticker {
                    name,
                    image_url,
                    category: category.to_string(),
                });
            }
        }

        Ok(())
    }

    /// 從 JSON 檔案載入貼圖資料
    pub fn load_json(&mut self, path: &str, category: &str) -> Result<()> {
        let content =
            fs::read_to_string(path).with_context(|| format!("無法讀取 JSON 檔案: {}", path))?;

        let json_data: HashMap<String, String> = serde_json::from_str(&content)
            .with_context(|| format!("解析 JSON 檔案時發生錯誤: {}", path))?;

        for (name, image_url) in json_data {
            self.stickers.push(Sticker {
                name,
                image_url,
                category: category.to_string(),
            });
        }

        Ok(())
    }

    /// 從配置載入所有貼圖資料
    pub fn load_from_config(config: &crate::config::StickersConfig) -> Result<Self> {
        let mut db = Self::new();

        for category_config in &config.categories {
            for csv_path in &category_config.csv {
                db.load_csv(csv_path, &category_config.name)
                    .with_context(|| format!("載入 CSV 檔案失敗: {}", csv_path))?;
            }

            for json_path in &category_config.json {
                db.load_json(json_path, &category_config.name)
                    .with_context(|| format!("載入 JSON 檔案失敗: {}", json_path))?;
            }
        }

        Ok(db)
    }

    /// 取得所有分類
    pub fn get_categories(&self) -> Vec<String> {
        let mut categories: Vec<String> =
            self.stickers.iter().map(|s| s.category.clone()).collect();
        categories.sort();
        categories.dedup();
        categories
    }

    /// 取得所有貼圖
    pub fn get_all(&self) -> &[Sticker] {
        &self.stickers
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
    pub fn search(&self, keyword: &str, categories: Option<&[String]>) -> Vec<&Sticker> {
        let (query_category, include_keywords, exclude_keywords) = Self::parse_query(keyword);

        self.stickers
            .iter()
            .filter(|s| {
                // 檢查分類過濾（優先使用查詢中指定的分類）
                let category_match = if let Some(ref cat) = query_category {
                    s.category.to_lowercase() == cat.to_lowercase()
                } else if let Some(cats) = categories {
                    cats.is_empty() || cats.contains(&s.category)
                } else {
                    true
                };

                if !category_match {
                    return false;
                }

                let name_lower = s.name.to_lowercase();

                // 檢查所有包含關鍵字（AND 條件）
                let include_match = include_keywords.is_empty()
                    || include_keywords.iter().all(|kw| name_lower.contains(kw));

                if !include_match {
                    return false;
                }

                // 檢查排除關鍵字（任一匹配則排除）
                let exclude_match = exclude_keywords.iter().any(|kw| name_lower.contains(kw));

                !exclude_match
            })
            .collect()
    }

    /// 根據索引取得貼圖
    pub fn get_by_index(&self, index: usize) -> Option<&Sticker> {
        self.stickers.get(index)
    }

    /// 取得貼圖數量
    pub fn count(&self) -> usize {
        self.stickers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn test_load_csv() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("test.csv");

        let csv_content = r#"日期,流水號,名稱,維基集數,ESFIO,無文字版本,imgur,i.imgur
191111,SS0001,你為什麼不問問神奇海螺呢,42-A,S3E03,,https://imgur.com/XB4MwpR,https://i.imgur.com/XB4MwpR.jpg
191111,SS0002,你現在是在懷疑神奇海螺的神奇魔力嗎,42-A,S3E03,,https://imgur.com/Mz19r2y,https://i.imgur.com/Mz19r2y.jpg"#;

        fs::write(&csv_path, csv_content).unwrap();

        let mut db = StickerDatabase::new();
        db.load_csv(csv_path.to_str().unwrap(), "測試分類").unwrap();

        assert_eq!(db.count(), 2);
        assert_eq!(db.get_by_index(0).unwrap().name, "你為什麼不問問神奇海螺呢");
        assert_eq!(db.get_by_index(0).unwrap().category, "測試分類");
        assert!(!db.get_by_index(0).unwrap().get_url_hash().is_empty());
    }

    #[test]
    fn test_load_csv_with_image_column() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("test.csv");

        // 測試使用「圖片」欄位
        let csv_content = r#"名稱,圖片,其他欄位
測試貼圖1,https://example.com/test1.jpg,test
測試貼圖2,https://example.com/test2.jpg,test"#;

        fs::write(&csv_path, csv_content).unwrap();

        let mut db = StickerDatabase::new();
        db.load_csv(csv_path.to_str().unwrap(), "其他").unwrap();

        assert_eq!(db.count(), 2);
        assert_eq!(db.get_by_index(0).unwrap().name, "測試貼圖1");
        assert_eq!(
            db.get_by_index(0).unwrap().image_url,
            "https://example.com/test1.jpg"
        );
        assert_eq!(db.get_by_index(0).unwrap().category, "其他");
    }

    #[test]
    fn test_load_json() {
        let temp_dir = TempDir::new().unwrap();
        let json_path = temp_dir.path().join("test.json");

        let json_content = r#"{
    "你很廉價": "https://i.imgur.com/gQRSLIx.png",
    "測試貼圖": "https://example.com/test.png"
}"#;

        fs::write(&json_path, json_content).unwrap();

        let mut db = StickerDatabase::new();
        db.load_json(json_path.to_str().unwrap(), "JSON分類")
            .unwrap();

        assert_eq!(db.count(), 2);
        assert!(db.get_all().iter().all(|s| s.category == "JSON分類"));
    }

    #[test]
    fn test_search() {
        let mut db = StickerDatabase::new();
        db.stickers.push(Sticker {
            name: "測試海螺".to_string(),
            image_url: "https://example.com/1.jpg".to_string(),
            category: "分類A".to_string(),
        });
        db.stickers.push(Sticker {
            name: "派大星".to_string(),
            image_url: "https://example.com/2.jpg".to_string(),
            category: "分類B".to_string(),
        });

        let results = db.search("海螺", None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "測試海螺");

        let results = db.search("", Some(&vec!["分類A".to_string()]));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "測試海螺");
    }

    #[test]
    fn test_search_advanced() {
        let mut db = StickerDatabase::new();
        db.stickers.push(Sticker {
            name: "開心派大星".to_string(),
            image_url: "https://example.com/1.jpg".to_string(),
            category: "海綿寶寶".to_string(),
        });
        db.stickers.push(Sticker {
            name: "難過派大星".to_string(),
            image_url: "https://example.com/2.jpg".to_string(),
            category: "海綿寶寶".to_string(),
        });
        db.stickers.push(Sticker {
            name: "開心章魚哥".to_string(),
            image_url: "https://example.com/3.jpg".to_string(),
            category: "海綿寶寶".to_string(),
        });
        db.stickers.push(Sticker {
            name: "開心小新".to_string(),
            image_url: "https://example.com/4.jpg".to_string(),
            category: "蠟筆小新".to_string(),
        });

        // 測試多關鍵字（AND 條件）
        let results = db.search("開心 派大星", None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "開心派大星");

        // 測試分類搜尋
        let results = db.search("海綿寶寶: 開心", None);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|s| s.name.contains("開心")));
        assert!(results.iter().all(|s| s.category == "海綿寶寶"));

        // 測試排除
        let results = db.search("開心 -派大星", None);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|s| !s.name.contains("派大星")));

        // 測試分類 + 排除
        let results = db.search("海綿寶寶: -章魚哥", None);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|s| s.category == "海綿寶寶"));
        assert!(results.iter().all(|s| !s.name.contains("章魚哥")));

        // 測試分類 + 多關鍵字 + 排除
        let results = db.search("海綿寶寶: 派大星 -難過", None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "開心派大星");
    }

    #[test]
    fn test_get_categories() {
        let mut db = StickerDatabase::new();
        db.stickers.push(Sticker {
            name: "測試1".to_string(),
            image_url: "https://example.com/1.jpg".to_string(),
            category: "分類A".to_string(),
        });
        db.stickers.push(Sticker {
            name: "測試2".to_string(),
            image_url: "https://example.com/2.jpg".to_string(),
            category: "分類B".to_string(),
        });
        db.stickers.push(Sticker {
            name: "測試3".to_string(),
            image_url: "https://example.com/3.jpg".to_string(),
            category: "分類A".to_string(),
        });

        let categories = db.get_categories();
        assert_eq!(categories.len(), 2);
        assert!(categories.contains(&"分類A".to_string()));
        assert!(categories.contains(&"分類B".to_string()));
    }
}
