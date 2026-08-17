-- Self-hosted kanban issue tables (metelix/rin-vibekanban)
-- Replaces the remote (Bloop cloud) issue backend with local SQLite storage.
-- Reuses the existing `projects` table (projects.id) as the project key.
-- Schema mirrors api_types structs (Issue, ProjectStatus, Tag, IssueAssignee,
-- IssueTag, IssueRelationship) so routes/remote handlers can be served
-- straight from local tables.
COMMIT;

PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS project_statuses (
    id          BLOB PRIMARY KEY,
    project_id  BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    color       TEXT NOT NULL DEFAULT '',
    sort_order  INTEGER NOT NULL DEFAULT 0,
    hidden      INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_project_statuses_project ON project_statuses(project_id);

CREATE TABLE IF NOT EXISTS issues (
    id                        BLOB PRIMARY KEY,
    project_id                BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    issue_number              INTEGER NOT NULL,
    simple_id                 TEXT NOT NULL,
    status_id                 BLOB NOT NULL REFERENCES project_statuses(id) ON DELETE RESTRICT,
    title                     TEXT NOT NULL,
    description               TEXT,
    priority                  TEXT CHECK (priority IN ('urgent','high','medium','low')),
    start_date                TEXT,
    target_date               TEXT,
    completed_at              TEXT,
    sort_order                REAL NOT NULL DEFAULT 0,
    parent_issue_id           BLOB REFERENCES issues(id) ON DELETE CASCADE,
    parent_issue_sort_order   REAL,
    extension_metadata        TEXT NOT NULL DEFAULT '{}',
    creator_user_id           BLOB,
    created_at                TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at                TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_issues_project ON issues(project_id);
CREATE INDEX IF NOT EXISTS idx_issues_status ON issues(status_id);
CREATE INDEX IF NOT EXISTS idx_issues_parent ON issues(parent_issue_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_issues_project_number ON issues(project_id, issue_number);
CREATE UNIQUE INDEX IF NOT EXISTS uq_issues_simple_id ON issues(simple_id);

CREATE TABLE IF NOT EXISTS issue_assignees (
    id          BLOB PRIMARY KEY,
    issue_id    BLOB NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    user_id     BLOB NOT NULL,
    assigned_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_issue_assignees_issue_user ON issue_assignees(issue_id, user_id);

CREATE TABLE IF NOT EXISTS kanban_tags (
    id          BLOB PRIMARY KEY,
    project_id  BLOB NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    color       TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_kanban_tags_project_name ON kanban_tags(project_id, name);

CREATE TABLE IF NOT EXISTS issue_tags (
    id          BLOB PRIMARY KEY,
    issue_id    BLOB NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    tag_id      BLOB NOT NULL REFERENCES kanban_tags(id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_issue_tags_issue_tag ON issue_tags(issue_id, tag_id);

CREATE TABLE IF NOT EXISTS issue_relationships (
    id                  BLOB PRIMARY KEY,
    issue_id            BLOB NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    related_issue_id    BLOB NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    relationship_type   TEXT NOT NULL,
    created_at          TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_issue_relationships_issue ON issue_relationships(issue_id);

PRAGMA foreign_key_check;

COMMIT;

PRAGMA foreign_keys = ON;

BEGIN TRANSACTION;