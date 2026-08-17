use std::str::FromStr;

use chrono::Duration;
use db::models::{auth_session, organization, user};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use uuid::Uuid;

async fn pool() -> SqlitePool {
    let url = format!(
        "sqlite:///{}-{}.sqlite",
        std::env::temp_dir().display(),
        Uuid::new_v4()
    );
    let options = SqliteConnectOptions::from_str(&url)
        .unwrap()
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn local_login_flow_roundtrip() {
    let pool = pool().await;
    let email = format!("user{}@rin.metelix.ai", Uuid::new_v4().simple());

    let u = user::upsert_user(&pool, &email, Some("alice"))
        .await
        .unwrap();
    assert_eq!(u.email, email);
    assert_eq!(u.username.as_deref(), Some("alice"));

    // Upsert is idempotent for the same email.
    let u2 = user::upsert_user(&pool, &email, Some("alice2"))
        .await
        .unwrap();
    assert_eq!(u2.id, u.id, "id must be stable across upsert");

    let by_id = user::find_user_by_id(&pool, u2.id).await.unwrap().unwrap();
    assert_eq!(by_id.id, u.id);
    let by_email = user::find_user_by_email(&pool, &email)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_email.id, u.id);

    // Personal org + admin membership.
    let org_id =
        organization::ensure_personal_org_and_admin_membership(&pool, u.id, u.username.as_deref())
            .await
            .unwrap();
    let org_id2 =
        organization::ensure_personal_org_and_admin_membership(&pool, u.id, u.username.as_deref())
            .await
            .unwrap();
    assert_eq!(org_id, org_id2, "personal org is stable");

    // Auth session create / find / rotate / invalidate.
    let session_id = Uuid::new_v4();
    let refresh_id = Uuid::new_v4().to_string();
    let s = auth_session::create(&pool, session_id, u.id, &refresh_id, Duration::days(365))
        .await
        .unwrap();
    assert_eq!(s.id, session_id);
    assert_eq!(s.user_id, u.id);

    let found = auth_session::find_by_id(&pool, session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.refresh_token_id.as_deref(), Some(refresh_id.as_str()));

    let by_ref = auth_session::find_by_refresh_token_id(&pool, &refresh_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_ref.id, session_id);

    assert!(
        auth_session::rotate_refresh_token(&pool, session_id, "new-refresh-id")
            .await
            .unwrap()
    );
    let after = auth_session::find_by_id(&pool, session_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.refresh_token_id.as_deref(), Some("new-refresh-id"));

    assert!(auth_session::invalidate(&pool, session_id).await.unwrap());
    assert!(
        auth_session::find_by_id(&pool, session_id)
            .await
            .unwrap()
            .is_none()
    );
}
