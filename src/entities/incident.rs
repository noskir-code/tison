//! The register of confidentiality incidents (Québec's Law 25 art. 3.8, its
//! regulation asking to keep each 5 years; GDPR art. 33.5): personal data
//! seen, lost or taken by someone who shouldn't have. Kept by the creator
//! of the site in Moderation > Incidents.

use sqlx::{
    PgPool, Row,
    types::chrono::{DateTime, NaiveDate, Utc},
};

use crate::entities::schema::{Column, Index, Table};

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "privacy_incidents",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        Column::new("occurred_on", "DATE").nullable(),
        Column::new("discovered_on", "DATE"),
        // What happened, and to which data (emails, messages...).
        Column::new("description", "TEXT"),
        Column::new("data_concerned", "TEXT").default("''"),
        // How many people, as known: "12", "about 300", "unknown".
        Column::new("people_affected", "TEXT").default("''"),
        // A risk of serious injury: the authority and the people are told.
        Column::new("serious_risk", "BOOLEAN").default("FALSE"),
        Column::new("measures", "TEXT").default("''"),
        Column::new("authority_notified_on", "DATE").nullable(),
        Column::new("people_notified_on", "DATE").nullable(),
        Column::new("created_by", "INTEGER").nullable().references("users(id) ON DELETE SET NULL"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
        Column::new("updated_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[Index::new("privacy_incidents_discovered_idx", "discovered_on DESC")],
};

/// Longest text of a field.
pub const TEXT_MAX_CHARS: usize = 5000;

pub struct Incident {
    pub id: i64,
    pub occurred_on: Option<NaiveDate>,
    pub discovered_on: NaiveDate,
    pub description: String,
    pub data_concerned: String,
    pub people_affected: String,
    pub serious_risk: bool,
    pub measures: String,
    pub authority_notified_on: Option<NaiveDate>,
    pub people_notified_on: Option<NaiveDate>,
    pub created_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// `2026-11-04` for the date fields of a form.
fn field(date: Option<NaiveDate>) -> String {
    date.map(|date| date.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

impl Incident {
    pub fn discovered_label(&self) -> String {
        crate::i18n::date(self.discovered_on)
    }

    pub fn updated_label(&self) -> String {
        crate::i18n::date_time(self.updated_at)
    }

    pub fn occurred_field(&self) -> String {
        field(self.occurred_on)
    }

    pub fn discovered_field(&self) -> String {
        field(Some(self.discovered_on))
    }

    pub fn authority_field(&self) -> String {
        field(self.authority_notified_on)
    }

    pub fn people_field(&self) -> String {
        field(self.people_notified_on)
    }

    /// A serious one not yet told to the authority or to the people.
    pub fn needs_notice(&self) -> bool {
        self.serious_risk && (self.authority_notified_on.is_none() || self.people_notified_on.is_none())
    }
}

/// What the form gives.
pub struct IncidentForm {
    pub occurred_on: Option<NaiveDate>,
    pub discovered_on: NaiveDate,
    pub description: String,
    pub data_concerned: String,
    pub people_affected: String,
    pub serious_risk: bool,
    pub measures: String,
    pub authority_notified_on: Option<NaiveDate>,
    pub people_notified_on: Option<NaiveDate>,
}

pub async fn all(pool: &PgPool) -> Result<Vec<Incident>, sqlx::Error> {
    sqlx::query(
        "SELECT i.*, u.username FROM privacy_incidents i LEFT JOIN users u ON u.id = i.created_by
         ORDER BY i.discovered_on DESC, i.id DESC",
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| {
        Ok(Incident {
            id: row.try_get("id")?,
            occurred_on: row.try_get("occurred_on")?,
            discovered_on: row.try_get("discovered_on")?,
            description: row.try_get("description")?,
            data_concerned: row.try_get("data_concerned")?,
            people_affected: row.try_get("people_affected")?,
            serious_risk: row.try_get("serious_risk")?,
            measures: row.try_get("measures")?,
            authority_notified_on: row.try_get("authority_notified_on")?,
            people_notified_on: row.try_get("people_notified_on")?,
            created_by: row.try_get("username")?,
            updated_at: row.try_get("updated_at")?,
        })
    })
    .collect()
}

/// Adds one (`id` none) or changes it.
pub async fn save(pool: &PgPool, id: Option<i64>, form: &IncidentForm, user_id: i32) -> Result<(), sqlx::Error> {
    let query = match id {
        Some(_) => {
            "UPDATE privacy_incidents SET occurred_on = $2, discovered_on = $3, description = $4, data_concerned = $5,
                people_affected = $6, serious_risk = $7, measures = $8, authority_notified_on = $9, people_notified_on = $10,
                updated_at = NOW()
             WHERE id = $1 AND $11::INTEGER IS NOT NULL"
        }
        None => {
            "INSERT INTO privacy_incidents (occurred_on, discovered_on, description, data_concerned, people_affected, serious_risk,
                measures, authority_notified_on, people_notified_on, created_by)
             SELECT $2, $3, $4, $5, $6, $7, $8, $9, $10, $11 WHERE $1::BIGINT IS NULL"
        }
    };
    sqlx::query(query)
        .bind(id)
        .bind(form.occurred_on)
        .bind(form.discovered_on)
        .bind(&form.description)
        .bind(&form.data_concerned)
        .bind(&form.people_affected)
        .bind(form.serious_risk)
        .bind(&form.measures)
        .bind(form.authority_notified_on)
        .bind(form.people_notified_on)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
