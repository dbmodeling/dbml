PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, email TEXT NOT NULL UNIQUE, status TEXT NOT NULL DEFAULT 'new' CHECK (status IN ('new','active','banned')), created TEXT DEFAULT (datetime('now')));
CREATE TABLE "posts" ("id" INTEGER NOT NULL, user_id INTEGER REFERENCES users(id) ON DELETE CASCADE, body, PRIMARY KEY("id"));
CREATE INDEX idx_posts_user ON posts(user_id);
COMMIT;
