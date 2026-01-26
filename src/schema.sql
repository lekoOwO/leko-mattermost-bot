CREATE TABLE IF NOT EXISTS group_buys (
    id TEXT PRIMARY KEY,
    creator_id TEXT NOT NULL,
    creator_username TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    post_id TEXT,
    merchant_name TEXT NOT NULL,
    description TEXT,
    metadata TEXT,
    items TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('active', 'closed')),
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS group_buy_orders (
    id TEXT PRIMARY KEY,
    group_buy_id TEXT NOT NULL,
    registrar_id TEXT NOT NULL,
    registrar_username TEXT NOT NULL,
    buyer_id TEXT NOT NULL,
    buyer_username TEXT NOT NULL,
    item_name TEXT NOT NULL,
    quantity INTEGER NOT NULL,
    original_quantity INTEGER,
    unit_price TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (group_buy_id) REFERENCES group_buys(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS group_buy_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    group_buy_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    username TEXT NOT NULL,
    action TEXT NOT NULL,
    details TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (group_buy_id) REFERENCES group_buys(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS shortage_adjustments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    group_buy_id TEXT NOT NULL,
    order_id TEXT NOT NULL,
    adjuster_id TEXT NOT NULL,
    adjuster_username TEXT NOT NULL,
    item_name TEXT NOT NULL,
    buyer_id TEXT NOT NULL,
    buyer_username TEXT NOT NULL,
    old_quantity INTEGER NOT NULL,
    new_quantity INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (group_buy_id) REFERENCES group_buys(id) ON DELETE CASCADE,
    FOREIGN KEY (order_id) REFERENCES group_buy_orders(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_orders_group_buy_id ON group_buy_orders(group_buy_id);
CREATE INDEX IF NOT EXISTS idx_orders_buyer_id ON group_buy_orders(buyer_id);
CREATE INDEX IF NOT EXISTS idx_logs_group_buy_id ON group_buy_logs(group_buy_id);

-- Stickers table: store sticker metadata to avoid loading all stickers into memory
CREATE TABLE IF NOT EXISTS stickers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    image_url TEXT NOT NULL UNIQUE,
    category TEXT NOT NULL,
    url_hash TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_stickers_category ON stickers(category);
CREATE INDEX IF NOT EXISTS idx_stickers_url_hash ON stickers(url_hash);

-- FTS5 virtual table for sticker name search. We populate this manually when inserting/replacing
-- stickers. We store ngram-like tokens in `name_ngrams` to improve Chinese search support.
-- (FTS removed) If full-text features are needed later, consider adding an FTS table
