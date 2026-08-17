# ローカルタスクDB化 実装計画書

## 目的

Vibe Kanban のタスク(issue)管理は現在、Bloop クラウドバックエンド(`RemoteClient`→遠隔
ElectricSQL/Postgres)に依存している。`RemoteClientNotConfigured` によりセルフホストでは
issue CRUD が動かない。

本改修は `/api/remote/*` ハンドラ群を**ローカル SQLite(`crates/db`)直結**に書き換え、
元の Bloop リモートバックエンドに一切依存せず、K8s の単一 SQLite で RiN のタスク管理を
完結させる。

## 現状アーキテクチャ

```
local-web フロントエンド
  │  /api/remote/issues ... を呼ぶ
  ▼
server (crates/server/src/routes/remote/*)
  │  各ハンドラが deployment.remote_client().await? を呼ぶ
  ▼
services::RemoteClient (HTTP+OAuth) ──▶ Bloop クラウド(遠隔DB)
  │
  ▼ (セルフホストでは)
deployment.remote_client() = Err(RemoteClientNotConfigured)
```

## 改修後アーキテクチャ

```
local-web フロントエンド
  │  /api/remote/issues ...（変更なし・互換維持）
  ▼
server (crates/server/src/routes/remote/*)
  │  各ハンドラが直接 SQLitePool を操作（remote_client() を通さない）
  ▼
crates/db（SQLite / migrations）
  └── issues / issue_assignees / issue_tags / issue_relationships / tags /
      project_statuses / issue_numbers テーブル（新規）
```

## スキーマ設計（新規テーブル migration）

既存 `projects`/`tasks` は「ローカルワークスペース実行タスク」用で、カンバン issue とは
別物。issue 管理はクリーンに新規テーブルで追加する（既存スキーマを壊さない）。

### `remote_projects`
| col | type |
|---|---|
| id | uuid PK (= projects.id 準拠、独立生成) |
| name | text NOT NULL |
| slug | text UNIQUE |
| created_at / updated_at | datetime |

### `project_statuses`
| col | type |
|---|---|
| id | uuid PK |
| project_id | uuid FK→remote_projects CASCADE |
| name | text |
| color | text default '' |
| position | integer default 0 |

シード: 全プロジェクト作成時に `Todo / In Progress / Done / Cancelled / In Review` を投入。

### `issues`
| col | type |
|---|---|
| id | uuid PK |
| project_id | uuid FK→remote_projects CASCADE |
| issue_number | integer（プロジェクト内通し番号） |
| simple_id | text（`{slug}-{issue_number}`） |
| status_id | uuid FK→project_statuses |
| title | text NOT NULL |
| description | text |
| priority | text NULL CHECK in ('urgent','high','medium','low') |
| start_date / target_date / completed_at | datetime NULL |
| sort_order | real NOT NULL default 0 |
| parent_issue_id | uuid NULL FK→issues(id) |
| parent_issue_sort_order | real NULL |
| extension_metadata | text(JSON) default '{}' |
| creator_user_id | uuid NULL |
| created_at / updated_at | datetime |

### 付随テーブル
- `issue_assignees(issue_id, user_id, created_at)` PK(issue_id,user_id)
- `tags(id, project_id, name, color)` / `issue_tags(issue_id, tag_id, created_at)`
- `issue_relationships(id, issue_id, related_issue_id, relationship_type, created_at)`
- `remote_pull_requests(issue_id, ...)`（PR 連携：後続ステップ）

## API 互換マップ

各 `routes/remote/*.rs` ハンドラの `client.xxx()` 呼び出しを、DB 実装関数に置換する:

| ハンドラ | 現状(client) → 置換後(DB) |
|---|---|
| projects::list_remote_projects | `SELECT * FROM remote_projects` |
| projects::get_remote_project | `WHERE id=?` |
| project_statuses::list_project_statuses | `WHERE project_id=? ORDER BY position` |
| issues::list_issues | issues JOIN statuses + total_count |
| issues::search_issues | 検索条件(priority/status/search/tag/assignee)付きクエリ |
| issues::get_issue / create / update / delete | CRUD + issue_number 採番 + simple_id 生成 |
| issue_assignees::* | issue_assignees CRUD |
| issue_tags::* / tags::* | tags / issue_tags CRUD |
| issue_relationships::* | issue_relationships CRUD |
| pull_requests::* | remote_pull_requests（後続） |

## 型（ts-rs）再生成

`api_types::Issue / CreateIssueRequest / UpdateIssueRequest / SearchIssuesRequest` は
既に存在。DB 実装の戻り値が `api_types::*` と一致するよう変換 layer を新設
(`crates/db/src/models/issue.rs` に `IssueRow`、routes で `api_types::Issue` へ変換)。

スキーマ変更後: `pnpm run generate-types` で `shared/remote-types.ts` 再生成（必要なら）。

## 実装ステップ

1. 【DB】migration 追加（上記テーブル）+ `crates/db/src/models/` にクエリ実装
2. 【DB】`deployment` に `sqlite_pool()` アクセサを追加（routes から利用可能に）
3. 【routes】projects / project_statuses を DB 直結に
4. 【routes】issues CRUD + list/search + issue_number/simple_id 採番
5. 【routes】assignees / tags / relationships
6. 【routes】pull_requests（後続、モック）
7. 【型】shared/remote-types.ts 再生成 + `pnpm run check`
8. 【ビルド】`cargo check` / Docker ビルド
9. 【デプロイ】rin-vibekanban-deploy の Dockerfile/branch を更新

## 非機能

- 認証：セルフホスト単一ユーザー前提。`creator_user_id` は固定値 or NULL。
- 競合：SQLite WAL。書き込みはシリアル化、単一 pod。
- マイグレーション：`sqlx` 既存管理に追従。

## リスク

- `remote_client()` を他箇所（workspaces 連携・MCP）も呼んでいるため、置換は
  routes/remote に限定し、他ルートへの影響を最小化する。
- フロントエンドの issue 表示が期待する API 形状（`ApiResponse<T>` ラッパ、フィールド）
  を維持する必要がある。