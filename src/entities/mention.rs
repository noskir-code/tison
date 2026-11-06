//! The people a post mentions (`@alice`, `@bob@mastodon.social`, see
//! `crate::mention`): told by a notification here, and given to other
//! servers as `Mention` tags, which deliver the post to them (see
//! `API::activities::post_note`). Filled when a post is published or edited
//! here (`handlers::mention`), and from the tags of posts received.

use sqlx::{PgPool, Row};

use crate::entities::schema::{Column, Index, Table};

/// Mentions resolved for one post at most: more is spam, Mastodon has a
/// limit too.
pub const MAX_PER_POST: usize = 20;

pub const TABLE: Table = Table {
    name: "post_mentions",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        Column::new("post_id", "BIGINT").references("posts(id) ON DELETE CASCADE"),
        // A user of this server, or an account of another one.
        Column::new("user_id", "INTEGER").nullable().references("users(id) ON DELETE CASCADE"),
        Column::new("actor_id", "BIGINT")
            .nullable()
            .references("federation_actors(id) ON DELETE CASCADE"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[
        Index::unique("post_mentions_unique_idx", "post_id, COALESCE(user_id, 0), COALESCE(actor_id, 0)"),
        Index::new("post_mentions_user_idx", "user_id"),
    ],
};

/// Makes `users` and `actors` the people the post mentions, the others
/// forgotten (an edit). Returns the users mentioned for the first time: the
/// ones to tell.
pub async fn replace(pool: &PgPool, post_id: i64, users: &[i32], actors: &[i64]) -> Result<Vec<i32>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "DELETE FROM post_mentions WHERE post_id = $1
           AND NOT (COALESCE(user_id = ANY($2), FALSE) OR COALESCE(actor_id = ANY($3), FALSE))",
    )
    .bind(post_id)
    .bind(users)
    .bind(actors)
    .execute(&mut *transaction)
    .await?;
    let new: Vec<i32> = sqlx::query_scalar(
        "INSERT INTO post_mentions (post_id, user_id) SELECT $1, u FROM UNNEST($2::integer[]) AS u
         ON CONFLICT DO NOTHING RETURNING user_id",
    )
    .bind(post_id)
    .bind(users)
    .fetch_all(&mut *transaction)
    .await?;
    sqlx::query("INSERT INTO post_mentions (post_id, actor_id) SELECT $1, a FROM UNNEST($2::bigint[]) AS a ON CONFLICT DO NOTHING")
        .bind(post_id)
        .bind(actors)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(new)
}

/// A person a post mentions, for other servers.
pub struct Mentioned {
    /// Their ActivityPub address.
    pub actor_url: String,
    /// `@alice@tison.social`, `@bob@mastodon.social`.
    pub handle: String,
    /// Where the post is delivered to them; `None` for users here.
    pub inbox: Option<String>,
}

/// The people the post mentions. Users here have their actor address made
/// of `local_actor_prefix` and their `public_id` (`https://x/ap/users/` and
/// `123`, see `API::Federation::actor_url`), their handle of `site_domain`.
pub async fn for_post(pool: &PgPool, post_id: i64, local_actor_prefix: &str, site_domain: &str) -> Result<Vec<Mentioned>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT u.public_id, u.username, a.actor_id, a.username AS remote_username, a.host,
                COALESCE(a.shared_inbox, a.inbox) AS inbox
         FROM post_mentions m
         LEFT JOIN users u ON u.id = m.user_id
         LEFT JOIN federation_actors a ON a.id = m.actor_id
         WHERE m.post_id = $1 AND (a.id IS NULL OR a.blocked_at IS NULL)
         ORDER BY m.id",
    )
    .bind(post_id)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(match row.try_get::<Option<String>, _>("actor_id")? {
                Some(actor_url) => Mentioned {
                    handle: format!("@{}@{}", row.try_get::<String, _>("remote_username")?, row.try_get::<String, _>("host")?),
                    inbox: row.try_get("inbox")?,
                    actor_url,
                },
                None => Mentioned {
                    actor_url: format!("{local_actor_prefix}{}", row.try_get::<i64, _>("public_id")?),
                    handle: format!("@{}@{site_domain}", row.try_get::<String, _>("username")?),
                    inbox: None,
                },
            })
        })
        .collect()
}
