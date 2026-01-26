use crate::sticker::Sticker;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Acquire;
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::info;

/// 資料庫連接池
#[derive(Clone, Debug)]
pub struct Database {
    pool: SqlitePool,
}

// Embedded canonical schema at compile time. This guarantees the running
// binary has a single source-of-truth and does not require `ci/schema.sql`
// to be present at runtime. CI and local tooling still use `ci/schema.sql`
// as the source for generation, but the binary embeds the same contents.
const EMBEDDED_SCHEMA: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/schema.sql"));

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::utils::{
        close_group_buy, create_and_insert_order, insert_group_buy, make_group_buy, setup_db,
    };
    use rust_decimal::Decimal;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_create_and_get_group_buy() {
        let db = setup_db().await;

        let gb = insert_group_buy(&db, 1).await;

        let fetched = db.get_group_buy(&gb.id).await.expect("get gb");
        assert!(fetched.is_some());
        let f = fetched.unwrap();
        assert_eq!(f.id, gb.id);
        assert_eq!(f.version, 1);
    }

    #[tokio::test]
    async fn test_update_items_and_version_conflict() {
        let db = setup_db().await;

        let gb = insert_group_buy(&db, 1).await;

        let mut new_items = std::collections::HashMap::new();
        new_items.insert("banana".to_string(), Decimal::new(500, 2));

        // success with correct version
        db.update_items(&gb.id, &new_items, 1, "u1", "u1")
            .await
            .expect("update items");
        let fetched = db.get_group_buy(&gb.id).await.unwrap().unwrap();
        assert_eq!(fetched.version, 2);

        // conflict when using old version
        let mut another = std::collections::HashMap::new();
        another.insert("pear".to_string(), Decimal::new(300, 2));

        let res = db.update_items(&gb.id, &another, 1, "u1", "u1").await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_update_post_id_and_status() {
        let db = setup_db().await;
        let id = Uuid::new_v4().to_string();
        let gb = make_group_buy(id.clone(), 1);
        db.create_group_buy(&gb).await.expect("create gb");

        db.update_post_id(&id, "post123")
            .await
            .expect("update post");
        let fetched = db.get_group_buy(&id).await.unwrap().unwrap();
        assert_eq!(fetched.post_id, Some("post123".to_string()));

        // update status to closed
        db.update_status(&id, GroupBuyStatus::Closed, 1, "u2", "u2")
            .await
            .expect("update status");
        let fetched2 = db.get_group_buy(&id).await.unwrap().unwrap();
        assert_eq!(fetched2.status, GroupBuyStatus::Closed);
        assert_eq!(fetched2.version, 2);
    }

    #[tokio::test]
    async fn test_create_order_and_queries() {
        let db = setup_db().await;
        let id = Uuid::new_v4().to_string();
        let gb = make_group_buy(id.clone(), 1);
        db.create_group_buy(&gb).await.expect("create gb");

        let _order = create_and_insert_order(&db, &gb.id, "buyer1", "reg1", 2).await;
        let orders = db
            .get_orders_by_group_buy(&gb.id)
            .await
            .expect("get orders");
        assert_eq!(orders.len(), 1);

        let buyer_orders = db
            .get_buyer_orders(&gb.id, "buyer1")
            .await
            .expect("get buyer orders");
        assert_eq!(buyer_orders.len(), 1);

        let all_orders = db.get_all_orders(&gb.id).await.expect("get all orders");
        assert_eq!(all_orders.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_buyer_item_and_delete_all() {
        let db = setup_db().await;

        // create a group_buy
        let gb = insert_group_buy(&db, 1).await;
        // create orders for buyer1 and buyer2
        let _order1 = create_and_insert_order(&db, &gb.id, "buyer1", "reg1", 2).await;
        let _order2 = create_and_insert_order(&db, &gb.id, "buyer2", "reg2", 2).await;

        // delete buyer1's apple orders
        let rows = db
            .delete_buyer_item_orders(&gb.id, "buyer1", "apple", "actor1", "actor1")
            .await
            .expect("delete buyer1 item");
        assert!(rows >= 1);

        // delete all orders for buyer2
        let rows2 = db
            .delete_orders_for_buyer(&gb.id, "buyer2", "actor2", "actor2")
            .await
            .expect("delete buyer2 all");
        assert!(rows2 >= 1);
    }

    #[tokio::test]
    async fn test_adjust_single_and_batch() {
        let db = setup_db().await;
        let gb = insert_group_buy(&db, 1).await;
        // create two orders
        let o1 = create_and_insert_order(&db, &gb.id, "alice", "reg1", 3).await;
        let _o2 = create_and_insert_order(&db, &gb.id, "bob", "reg2", 4).await;

        // close the group buy so adjustments are allowed
        close_group_buy(&db, &gb.id, 1).await;

        // adjust single order
        db.adjust_single_order(&o1.id, 1, "adj", "adj")
            .await
            .expect("adjust single");
        let orders = db.get_all_orders(&gb.id).await.expect("get orders");
        let o1_after = orders.iter().find(|o| o.id == o1.id).unwrap();
        assert_eq!(o1_after.quantity, 1);

        // batch adjust
        let mut map = std::collections::HashMap::new();
        map.insert("bob".to_string(), 2);
        let records = db
            .adjust_order_quantity(&gb.id, "apple", &map, "adj2", "adj2")
            .await
            .expect("batch adjust");
        assert_eq!(records.len(), 1);

        // ensure shortage_adjustments rows exist
        let cnt: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM shortage_adjustments WHERE group_buy_id = ?")
                .bind(&gb.id)
                .fetch_one(&db.pool)
                .await
                .expect("count adjustments");
        assert!(cnt >= 1);
    }

    #[tokio::test]
    async fn test_sticker_bulk_insert_and_search() {
        use crate::sticker::Sticker;

        let db = setup_db().await;

        let stickers = vec![
            Sticker {
                name: "apple smile".to_string(),
                image_url: "https://example.com/a1.png".to_string(),
                category: "fruit".to_string(),
            },
            Sticker {
                name: "banana happy".to_string(),
                image_url: "https://example.com/b1.png".to_string(),
                category: "fruit".to_string(),
            },
            Sticker {
                name: "carrot".to_string(),
                image_url: "https://example.com/c1.png".to_string(),
                category: "veg".to_string(),
            },
        ];

        let inserted = db
            .bulk_insert_stickers(&stickers)
            .await
            .expect("bulk insert");
        assert!(inserted >= 3);

        let cnt = db.count_stickers().await.expect("count");
        assert!(cnt >= 3);

        // search for "apple"
        let res = db
            .search_stickers(None, &vec!["apple".to_string()], &vec![], None, 10)
            .await
            .expect("search");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].name, "apple smile");
    }
}

impl Database {
    /// 初始化資料庫連接
    pub async fn new(database_url: &str) -> Result<Self> {
        // 解析 connection string
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5))
            .auto_vacuum(sqlx::sqlite::SqliteAutoVacuum::Full)
            .foreign_keys(true);

        // 建立連接池
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .with_context(|| format!("無法連接到資料庫: {}", database_url))?;

        let db = Database { pool };

        // 初始化資料表
        db.init_schema().await?;

        info!("資料庫初始化成功: {}", database_url);

        Ok(db)
    }

    /// 建立資料表結構
    async fn init_schema(&self) -> Result<()> {
        // Prefer a single source-of-truth schema file when explicitly set via
        // `DB_SCHEMA_FILE`. Otherwise use the embedded schema that is baked
        // into the binary at compile time (see `EMBEDDED_SCHEMA`). This keeps
        // runtime self-contained.
        if let Ok(schema_path) = std::env::var("DB_SCHEMA_FILE") {
            if let Ok(schema) = std::fs::read_to_string(&schema_path) {
                for stmt in schema.split(';') {
                    let s = stmt.trim();
                    if s.is_empty() {
                        continue;
                    }
                    sqlx::query(s).execute(&self.pool).await?;
                }
                info!("資料表結構初始化完成 (from {})", schema_path);
                return Ok(());
            } else {
                info!(
                    "DB_SCHEMA_FILE set but not readable: {}. Falling back to embedded schema",
                    schema_path
                );
            }
        }

        // No external schema provided or readable — apply the embedded schema.
        self.apply_embedded_schema().await?;

        Ok(())
    }

    /// Apply the embedded schema (EMBEDDED_SCHEMA) to the database. This is
    /// the fallback path and is also the recommended runtime behavior so the
    /// binary does not require external SQL files.
    async fn apply_embedded_schema(&self) -> Result<()> {
        for stmt in EMBEDDED_SCHEMA.split(';') {
            let s = stmt.trim();
            if s.is_empty() {
                continue;
            }
            sqlx::query(s).execute(&self.pool).await?;
        }
        info!("資料表結構初始化完成 (embedded)");

        Ok(())
    }

    /* ---------- Sticker helpers ---------- */

    /// Bulk insert stickers into the stickers table (INSERT OR IGNORE to avoid duplicates)
    pub async fn bulk_insert_stickers(&self, stickers: &[Sticker]) -> Result<usize> {
        let mut inserted: usize = 0;
        // Acquire a dedicated connection and start a transaction for bulk insert
        let mut conn = self.pool.acquire().await?;
        let mut tx = conn.begin().await?;

        for s in stickers {
            let url_hash = s.get_url_hash();
            let created_at = Utc::now().to_rfc3339();
            let res = sqlx::query(
                "INSERT OR IGNORE INTO stickers (name, image_url, category, url_hash, created_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&s.name)
            .bind(&s.image_url)
            .bind(&s.category)
            .bind(&url_hash)
            .bind(&created_at)
            .execute(&mut *tx)
            .await?;

            if res.rows_affected() > 0 {
                inserted += 1;
            }
        }
        // no FTS population — using LIKE-based searches instead

        tx.commit().await?;
        Ok(inserted)
    }

    /// Replace all stickers atomically: delete existing rows and insert the provided list.
    /// Returns number of inserted rows.
    pub async fn replace_stickers(&self, stickers: &[Sticker]) -> Result<usize> {
        let mut inserted: usize = 0;
        let mut conn = self.pool.acquire().await?;
        let mut tx = conn.begin().await?;

        // Clear existing stickers
        sqlx::query("DELETE FROM stickers")
            .execute(&mut *tx)
            .await?;

        for s in stickers {
            let url_hash = s.get_url_hash();
            let created_at = Utc::now().to_rfc3339();
            let res = sqlx::query(
                "INSERT OR IGNORE INTO stickers (name, image_url, category, url_hash, created_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&s.name)
            .bind(&s.image_url)
            .bind(&s.category)
            .bind(&url_hash)
            .bind(&created_at)
            .execute(&mut *tx)
            .await?;

            if res.rows_affected() > 0 {
                inserted += 1;
            }
        }

        // no FTS population during replace — using LIKE-based searches instead

        tx.commit().await?;
        Ok(inserted)
    }

    /// Count total stickers
    pub async fn count_stickers(&self) -> Result<i64> {
        let cnt: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stickers")
            .fetch_one(&self.pool)
            .await?;
        Ok(cnt)
    }

    /// Get category statistics (category -> count)
    pub async fn get_sticker_category_stats(&self) -> Result<HashMap<String, i64>> {
        let rows = sqlx::query("SELECT category, COUNT(*) as cnt FROM stickers GROUP BY category")
            .fetch_all(&self.pool)
            .await?;

        let mut map = HashMap::new();
        for r in rows {
            let category: String = r.try_get("category")?;
            let cnt: i64 = r.try_get("cnt")?;
            map.insert(category, cnt);
        }
        Ok(map)
    }

    /// Search stickers with include/exclude keywords and optional category filters.
    pub async fn search_stickers(
        &self,
        opt_category: Option<&str>,
        include_keywords: &[String],
        exclude_keywords: &[String],
        categories_filter: Option<&[String]>,
        limit: i64,
    ) -> Result<Vec<Sticker>> {
        let mut sql = String::from("SELECT name, image_url, category FROM stickers");
        let mut where_clauses: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(cat) = opt_category {
            where_clauses.push("LOWER(category) = LOWER(?)".to_string());
            binds.push(cat.to_string());
        } else if let Some(cats) = categories_filter {
            if !cats.is_empty() {
                let placeholders = cats.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                where_clauses.push(format!("category IN ({})", placeholders));
                for c in cats.iter() {
                    binds.push(c.clone());
                }
            }
        }

        for kw in include_keywords.iter() {
            where_clauses.push("LOWER(name) LIKE LOWER(?)".to_string());
            binds.push(format!("%{}%", kw));
        }

        if !exclude_keywords.is_empty() {
            let mut exs: Vec<String> = Vec::new();
            for _ in exclude_keywords.iter() {
                exs.push("LOWER(name) LIKE LOWER(?)".to_string());
            }
            where_clauses.push(format!("NOT ({})", exs.join(" OR ")));
            for kw in exclude_keywords.iter() {
                binds.push(format!("%{}%", kw));
            }
        }

        if !where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_clauses.join(" AND "));
        }

        sql.push_str(" ORDER BY category, name LIMIT ?");

        let mut q = sqlx::query(&sql);
        for b in binds.iter() {
            q = q.bind(b);
        }
        q = q.bind(limit);

        let rows = q.fetch_all(&self.pool).await?;

        let mut stickers_out: Vec<Sticker> = Vec::new();
        for r in rows {
            let name: String = r.try_get("name")?;
            let image_url: String = r.try_get("image_url")?;
            let category: String = r.try_get("category")?;
            stickers_out.push(Sticker {
                name,
                image_url,
                category,
            });
        }

        Ok(stickers_out)
    }

    /// 記錄操作日誌
    pub async fn log_action(
        &self,
        group_buy_id: &str,
        user_id: &str,
        username: &str,
        action: &str,
        details: Option<&str>,
    ) -> Result<()> {
        // Use the provided `details` string as-is. Callers are responsible for
        // supplying a minified JSON string that includes a "version" key.
        // If None is provided, record an empty JSON object.
        let details_min = details
            .map(|s| s.to_string())
            .unwrap_or_else(|| "{}".to_string());

        let created = Utc::now().to_rfc3339();
        sqlx::query!(
            "INSERT INTO group_buy_logs (group_buy_id, user_id, username, action, details, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            group_buy_id,
            user_id,
            username,
            action,
            details_min,
            created
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 建立新團購
    pub async fn create_group_buy(&self, group_buy: &GroupBuy) -> Result<()> {
        let metadata_json = serde_json::to_string(&group_buy.metadata)?;
        let items_json = serde_json::to_string(&group_buy.items)?;

        // materialize owned values for sqlx macros
        let gb_id = group_buy.id.clone();
        let gb_creator_id = group_buy.creator_id.clone();
        let gb_creator_username = group_buy.creator_username.clone();
        let gb_channel_id = group_buy.channel_id.clone();
        let gb_post_id = group_buy.post_id.clone();
        let gb_merchant_name = group_buy.merchant_name.clone();
        let gb_description = group_buy.description.clone();
        let gb_status = group_buy.status.to_string();
        let gb_created_at = group_buy.created_at.to_rfc3339();
        let gb_updated_at = group_buy.updated_at.to_rfc3339();

        sqlx::query!(
            "INSERT INTO group_buys (
                id, creator_id, creator_username, channel_id, post_id,
                merchant_name, description, metadata, items, status,
                version, created_at, updated_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            gb_id,
            gb_creator_id,
            gb_creator_username,
            gb_channel_id,
            gb_post_id,
            gb_merchant_name,
            gb_description,
            metadata_json,
            items_json,
            gb_status,
            group_buy.version,
            gb_created_at,
            gb_updated_at
        )
        .execute(&self.pool)
        .await?;

        // details must be JSON with version key (minified)
        let details_json = serde_json::json!({
            "merchant_name": group_buy.merchant_name,
            "action": "create",
            "version": group_buy.version,
        });
        let details = serde_json::to_string(&details_json)?;
        self.log_action(
            &group_buy.id,
            &group_buy.creator_id,
            &group_buy.creator_username,
            "create",
            Some(&details),
        )
        .await?;

        Ok(())
    }

    /// 取得團購資料
    pub async fn get_group_buy(&self, id: &str) -> Result<Option<GroupBuy>> {
        let result = sqlx::query_as!(
            GroupBuyRow,
            "SELECT id, creator_id, creator_username, channel_id, post_id,
                    merchant_name, description, metadata, items, status,
                    version, created_at, updated_at
             FROM group_buys WHERE id = ?",
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|row| row.into()))
    }

    /// 更新團購商品列表
    pub async fn update_items(
        &self,
        id: &str,
        items: &HashMap<String, Decimal>,
        expected_version: i32,
        user_id: &str,
        username: &str,
    ) -> Result<()> {
        let items_json = serde_json::to_string(items)?;

        let updated_at = Utc::now().to_rfc3339();
        let result = sqlx::query!(
            "UPDATE group_buys 
             SET items = ?, version = version + 1, updated_at = ?
             WHERE id = ? AND version = ? AND status = 'active'",
            items_json,
            updated_at,
            id,
            expected_version
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            anyhow::bail!("更新失敗：團購已被修改或已截止，請重新整理");
        }

        let details_json = serde_json::json!({
            "items_count": items.len(),
            "action": "update_items",
            "version": expected_version,
        });
        let details = serde_json::to_string(&details_json)?;
        self.log_action(id, user_id, username, "update_items", Some(&details))
            .await?;

        Ok(())
    }

    /// 更新團購的 post_id（第一次按鈕點擊時使用）
    pub async fn update_post_id(&self, id: &str, post_id: &str) -> Result<()> {
        let updated_at = Utc::now().to_rfc3339();
        let result = sqlx::query!(
            "UPDATE group_buys 
             SET post_id = ?, updated_at = ?
             WHERE id = ?",
            post_id,
            updated_at,
            id
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            anyhow::bail!("更新 post_id 失敗：找不到該團購");
        }

        Ok(())
    }

    /// 更新團購狀態
    pub async fn update_status(
        &self,
        id: &str,
        status: GroupBuyStatus,
        expected_version: i32,
        user_id: &str,
        username: &str,
    ) -> Result<()> {
        let status_str = status.to_string();
        let updated_at = Utc::now().to_rfc3339();
        let result = sqlx::query!(
            "UPDATE group_buys 
             SET status = ?, version = version + 1, updated_at = ?
             WHERE id = ? AND version = ?",
            status_str,
            updated_at,
            id,
            expected_version
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            anyhow::bail!("更新失敗：團購狀態已變更，請重新整理");
        }

        let details_json = serde_json::json!({
            "new_status": status.to_string(),
            "action": format!("update_status_{}", status),
            "version": expected_version,
        });
        let details = serde_json::to_string(&details_json)?;
        self.log_action(
            id,
            user_id,
            username,
            &format!("update_status_{}", status),
            Some(&details),
        )
        .await?;

        Ok(())
    }

    /// 新增訂單
    pub async fn create_order(&self, order: &GroupBuyOrder) -> Result<()> {
        // 檢查團購狀態
        let status: String = sqlx::query_scalar!(
            "SELECT status FROM group_buys WHERE id = ?",
            order.group_buy_id
        )
        .fetch_one(&self.pool)
        .await?;

        if status != "active" {
            anyhow::bail!("團購已截止，無法登記");
        }

        // Materialize temporary values as locals so they live long enough for
        // the sqlx macro expansion / execution and to avoid temporary-borrow
        // lifetime issues.
        let id = order.id.clone();
        let group_buy_id = order.group_buy_id.clone();
        let registrar_id = order.registrar_id.clone();
        let registrar_username = order.registrar_username.clone();
        let buyer_id = order.buyer_id.clone();
        let buyer_username = order.buyer_username.clone();
        let item_name = order.item_name.clone();
        let quantity = order.quantity as i64;
        let original_quantity = order.original_quantity.map(|v| v as i64);
        let unit_price = order.unit_price.to_string(); // 將 Decimal 轉為字串儲存
        let created_at = order.created_at.to_rfc3339();

        sqlx::query!(
            "INSERT INTO group_buy_orders (
                id, group_buy_id, registrar_id, registrar_username,
                buyer_id, buyer_username, item_name, quantity,
                original_quantity, unit_price, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            id,
            group_buy_id,
            registrar_id,
            registrar_username,
            buyer_id,
            buyer_username,
            item_name,
            quantity,
            original_quantity,
            unit_price,
            created_at
        )
        .execute(&self.pool)
        .await?;

        // fetch current version for the group_buy
        let version: i64 = sqlx::query_scalar!(
            "SELECT version FROM group_buys WHERE id = ?",
            order.group_buy_id
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0i64);

        let details_json = serde_json::json!({
            "buyer": order.buyer_username,
            "item": order.item_name,
            "quantity": order.quantity,
            "action": "register",
            "version": version as i32,
        });
        let details = serde_json::to_string(&details_json)?;
        self.log_action(
            &order.group_buy_id,
            &order.registrar_id,
            &order.registrar_username,
            "register",
            Some(&details),
        )
        .await?;

        Ok(())
    }

    /// 取得團購的所有訂單
    pub async fn get_orders_by_group_buy(&self, group_buy_id: &str) -> Result<Vec<GroupBuyOrder>> {
        let rows = sqlx::query_as!(
            GroupBuyOrderRow,
            "SELECT id, group_buy_id, registrar_id, registrar_username,
                    buyer_id, buyer_username, item_name, quantity,
                    original_quantity, unit_price, created_at
             FROM group_buy_orders
             WHERE group_buy_id = ?
             ORDER BY created_at ASC",
            group_buy_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    /// 刪除特定買家在特定商品的所有訂單（用於數量為 0 的情況）
    pub async fn delete_buyer_item_orders(
        &self,
        group_buy_id: &str,
        buyer_id: &str,
        item_name: &str,
        actor_id: &str,
        actor_username: &str,
    ) -> Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM group_buy_orders WHERE group_buy_id = ? AND buyer_id = ? AND item_name = ?",
            group_buy_id,
            buyer_id,
            item_name
        )
        .execute(&self.pool)
        .await?;

        // 記錄日誌，使用操作人的資訊（details 為 JSON，含 version）
        let version: i64 =
            sqlx::query_scalar!("SELECT version FROM group_buys WHERE id = ?", group_buy_id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0i64);
        let details_json = serde_json::json!({
            "buyer_id": buyer_id,
            "item_name": item_name,
            "action": "delete_registration",
            "version": version as i32,
        });
        let details = serde_json::to_string(&details_json).unwrap_or_else(|_| "{}".to_string());
        let _ = self
            .log_action(
                group_buy_id,
                actor_id,
                actor_username,
                "delete_registration",
                Some(&details),
            )
            .await;

        Ok(result.rows_affected())
    }

    /// 刪除特定買家的所有訂單（用於取消登記功能）
    pub async fn delete_orders_for_buyer(
        &self,
        group_buy_id: &str,
        buyer_id: &str,
        actor_id: &str,
        actor_username: &str,
    ) -> Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM group_buy_orders WHERE group_buy_id = ? AND buyer_id = ?",
            group_buy_id,
            buyer_id
        )
        .execute(&self.pool)
        .await?;

        // 記錄日誌（details 為 JSON，含 version）
        let version: i64 =
            sqlx::query_scalar!("SELECT version FROM group_buys WHERE id = ?", group_buy_id)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0i64);
        let details_json = serde_json::json!({
            "buyer_id": buyer_id,
            "action": "cancel_all_registrations",
            "version": version as i32,
        });
        let details = serde_json::to_string(&details_json).unwrap_or_else(|_| "{}".to_string());
        let _ = self
            .log_action(
                group_buy_id,
                actor_id,
                actor_username,
                "cancel_all_registrations",
                Some(&details),
            )
            .await;

        Ok(result.rows_affected())
    }

    /// 調整訂單數量（缺貨時使用）
    pub async fn get_buyer_orders(
        &self,
        group_buy_id: &str,
        buyer_id: &str,
    ) -> Result<Vec<GroupBuyOrder>> {
        let orders = sqlx::query_as!(
            GroupBuyOrderRow,
            "SELECT id, group_buy_id, registrar_id, registrar_username,
                    buyer_id, buyer_username, item_name, quantity,
                    original_quantity, unit_price, created_at
             FROM group_buy_orders
             WHERE group_buy_id = ? AND buyer_id = ?",
            group_buy_id,
            buyer_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(orders.into_iter().map(|row| row.into()).collect())
    }

    /// 取得所有訂單
    pub async fn get_all_orders(&self, group_buy_id: &str) -> Result<Vec<GroupBuyOrder>> {
        let orders = sqlx::query_as!(
            GroupBuyOrderRow,
            "SELECT id, group_buy_id, registrar_id, registrar_username,
                    buyer_id, buyer_username, item_name, quantity,
                    original_quantity, unit_price, created_at
             FROM group_buy_orders
             WHERE group_buy_id = ?",
            group_buy_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(orders.into_iter().map(|row| row.into()).collect())
    }

    /// 調整單個訂單的數量
    pub async fn adjust_single_order(
        &self,
        order_id: &str,
        new_quantity: i32,
        adjuster_id: &str,
        adjuster_username: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // 取得訂單資訊
        let order = sqlx::query_as!(
            GroupBuyOrderRow,
            "SELECT id, group_buy_id, registrar_id, registrar_username,
            buyer_id, buyer_username, item_name, quantity,
            original_quantity, unit_price, created_at
         FROM group_buy_orders
         WHERE id = ?",
            order_id
        )
        .fetch_one(&mut *tx)
        .await?;

        // 檢查團購狀態
        let order_group_buy_id = order.group_buy_id.clone();
        let status: String = sqlx::query_scalar!(
            "SELECT status FROM group_buys WHERE id = ?",
            order_group_buy_id
        )
        .fetch_one(&mut *tx)
        .await?;

        if status != "closed" {
            anyhow::bail!("只能在團購截止後調整缺貨");
        }

        let old_qty = order.quantity;
        let orig_qty = order.original_quantity.unwrap_or(old_qty);

        // materialize order-related locals to avoid temporary-borrow issues
        let order_group_buy_id = order.group_buy_id.clone();
        let order_item_name = order.item_name.clone();
        let order_buyer_id = order.buyer_id.clone();
        let order_buyer_username = order.buyer_username.clone();
        let order_id_clone = order.id.clone();

        // 更新訂單數量
        let new_qty_i64 = new_quantity as i64;
        sqlx::query!(
            "UPDATE group_buy_orders 
             SET quantity = ?, original_quantity = ?
             WHERE id = ?",
            new_qty_i64,
            orig_qty,
            order_id_clone
        )
        .execute(&mut *tx)
        .await?;

        // 記錄調整歷史
        let now = Utc::now().to_rfc3339();
        let now_for_insert = now.clone();
        sqlx::query!(
            "INSERT INTO shortage_adjustments (
                group_buy_id, order_id, adjuster_id, adjuster_username,
                item_name, buyer_id, buyer_username, old_quantity, new_quantity, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            order_group_buy_id,
            order_id_clone,
            adjuster_id,
            adjuster_username,
            order_item_name,
            order_buyer_id,
            order_buyer_username,
            old_qty,
            new_qty_i64,
            now_for_insert
        )
        .execute(&mut *tx)
        .await?;

        // 記錄日誌
        let msg = format!(
            "調整 @{} 的 {} 數量：{} → {}",
            order_buyer_username, order_item_name, old_qty, new_quantity
        );
        sqlx::query!(
            "INSERT INTO group_buy_logs (group_buy_id, user_id, username, action, details, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            order_group_buy_id,
            adjuster_id,
            adjuster_username,
            "adjust_shortage",
            msg,
            now
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// 調整訂單數量（缺貨調整）
    pub async fn adjust_order_quantity(
        &self,
        group_buy_id: &str,
        item_name: &str,
        adjustments: &HashMap<String, i32>,
        adjuster_id: &str,
        adjuster_username: &str,
    ) -> Result<Vec<AdjustmentRecord>> {
        let mut tx = self.pool.begin().await?;

        // 檢查團購狀態必須是 closed
        let status: String =
            sqlx::query_scalar!("SELECT status FROM group_buys WHERE id = ?", group_buy_id)
                .fetch_one(&mut *tx)
                .await?;

        if status != "closed" {
            anyhow::bail!("只能在團購截止後調整缺貨");
        }

        // 取得所有相關訂單
        let orders = sqlx::query_as!(
            OrderAdjustmentRow,
            "SELECT id, buyer_id, buyer_username, quantity, original_quantity
             FROM group_buy_orders
             WHERE group_buy_id = ? AND item_name = ?",
            group_buy_id,
            item_name
        )
        .fetch_all(&mut *tx)
        .await?;

        let mut records = Vec::new();

        for order in orders {
            // Skip orders without a buyer_username (shouldn't normally happen)
            let buyer_username = match order.buyer_username.clone() {
                Some(s) => s,
                None => continue,
            };

            if let Some(&new_qty) = adjustments.get(&buyer_username) {
                let old_qty = order.quantity;
                let orig_qty = order.original_quantity.unwrap_or(old_qty);

                // Ensure we have an order id to update; skip otherwise
                let order_id_clone = match order.id.clone() {
                    Some(s) => s,
                    None => continue,
                };

                // buyer_id may be absent in edge cases; use empty string if missing
                let order_buyer_id = order.buyer_id.clone().unwrap_or_default();
                let order_buyer_username = buyer_username.clone();
                let new_qty_i64 = new_qty as i64;

                sqlx::query!(
                    "UPDATE group_buy_orders 
                     SET quantity = ?, original_quantity = ?
                     WHERE id = ?",
                    new_qty_i64,
                    orig_qty,
                    order_id_clone
                )
                .execute(&mut *tx)
                .await?;

                // 記錄調整歷史
                let now = Utc::now().to_rfc3339();
                let now_for_insert = now.clone();
                sqlx::query!(
                    "INSERT INTO shortage_adjustments (
                        group_buy_id, order_id, adjuster_id, adjuster_username,
                        item_name, buyer_id, buyer_username, old_quantity, new_quantity, created_at
                     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    group_buy_id,
                    order_id_clone,
                    adjuster_id,
                    adjuster_username,
                    item_name,
                    order_buyer_id,
                    order_buyer_username,
                    old_qty,
                    new_qty_i64,
                    now_for_insert
                )
                .execute(&mut *tx)
                .await?;

                records.push(AdjustmentRecord {
                    buyer_username: order_buyer_username.clone(),
                    old_quantity: old_qty as i32,
                    new_quantity: new_qty,
                });
            }
        }

        // 記錄日誌（在同一交易中插入以避免連線/鎖定問題）
        let now2 = Utc::now().to_rfc3339();
        let details = format!("調整 {} 的數量，影響 {} 位用戶", item_name, records.len());
        sqlx::query!(
            "INSERT INTO group_buy_logs (group_buy_id, user_id, username, action, details, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            group_buy_id,
            adjuster_id,
            adjuster_username,
            "adjust_shortage",
            details,
            now2
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(records)
    }
}

// 資料結構定義

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupBuy {
    pub id: String,
    pub creator_id: String,
    pub creator_username: String,
    pub channel_id: String,
    pub post_id: Option<String>, // 第一次按鈕點擊時會更新
    pub merchant_name: String,
    pub description: Option<String>,
    pub metadata: HashMap<String, String>,
    pub items: HashMap<String, Decimal>, // 改用 Decimal 存儲價格
    pub status: GroupBuyStatus,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GroupBuyStatus {
    Active,
    Closed,
}

use std::fmt;

impl fmt::Display for GroupBuyStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroupBuyStatus::Active => write!(f, "active"),
            GroupBuyStatus::Closed => write!(f, "closed"),
        }
    }
}

impl GroupBuyStatus {
    pub fn from_string(s: &str) -> Self {
        match s {
            "closed" => GroupBuyStatus::Closed,
            _ => GroupBuyStatus::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupBuyOrder {
    pub id: String,
    pub group_buy_id: String,
    pub registrar_id: String,
    pub registrar_username: String,
    pub buyer_id: String,
    pub buyer_username: String,
    pub item_name: String,
    pub quantity: i32,
    pub original_quantity: Option<i32>,
    pub unit_price: Decimal, // 改用 Decimal 存儲單價
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjustmentRecord {
    pub buyer_username: String,
    pub old_quantity: i32,
    pub new_quantity: i32,
}

// SQLx Row 映射結構

#[derive(sqlx::FromRow)]
struct GroupBuyRow {
    id: Option<String>,
    creator_id: String,
    creator_username: String,
    channel_id: String,
    post_id: Option<String>,
    merchant_name: String,
    description: Option<String>,
    metadata: Option<String>,
    items: String,
    status: String,
    version: i64,
    created_at: String,
    updated_at: String,
}

impl From<GroupBuyRow> for GroupBuy {
    fn from(row: GroupBuyRow) -> Self {
        GroupBuy {
            id: row.id.unwrap_or_default(),
            creator_id: row.creator_id,
            creator_username: row.creator_username,
            channel_id: row.channel_id,
            post_id: row.post_id,
            merchant_name: row.merchant_name,
            description: row.description,
            metadata: row
                .metadata
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default(),
            items: serde_json::from_str(&row.items).unwrap_or_default(),
            status: GroupBuyStatus::from_string(&row.status),
            version: row.version as i32,
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .unwrap()
                .with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
                .unwrap()
                .with_timezone(&Utc),
        }
    }
}

#[derive(sqlx::FromRow)]
struct GroupBuyOrderRow {
    id: Option<String>,
    group_buy_id: String,
    registrar_id: String,
    registrar_username: String,
    buyer_id: String,
    buyer_username: String,
    item_name: String,
    quantity: i64,
    original_quantity: Option<i64>,
    unit_price: String, // 從資料庫讀取為字串
    created_at: String,
}

impl From<GroupBuyOrderRow> for GroupBuyOrder {
    fn from(row: GroupBuyOrderRow) -> Self {
        GroupBuyOrder {
            id: row.id.unwrap_or_default(),
            group_buy_id: row.group_buy_id,
            registrar_id: row.registrar_id,
            registrar_username: row.registrar_username,
            buyer_id: row.buyer_id,
            buyer_username: row.buyer_username,
            item_name: row.item_name,
            quantity: row.quantity as i32,
            original_quantity: row.original_quantity.map(|v| v as i32),
            unit_price: Decimal::from_str(&row.unit_price).unwrap_or(Decimal::ZERO), // 從字串解析回 Decimal
            created_at: DateTime::parse_from_rfc3339(&row.created_at)
                .unwrap()
                .with_timezone(&Utc),
        }
    }
}

#[derive(sqlx::FromRow)]
struct OrderAdjustmentRow {
    id: Option<String>,
    buyer_id: Option<String>,
    buyer_username: Option<String>,
    quantity: i64,
    original_quantity: Option<i64>,
}
