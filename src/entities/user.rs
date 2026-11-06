use sqlx::{
    PgPool, Row,
    postgres::PgRow,
    types::chrono::{DateTime, NaiveDate, Utc},
};

use crate::entities::schema::{Column, Index, Table};

pub const USERNAME_MIN_LEN: usize = 3;
pub const USERNAME_MAX_LEN: usize = 30;
pub const NAME_MAX_CHARS: usize = 64;
/// Same limit as Mastodon.
pub const BIO_MAX_CHARS: usize = 500;
pub const EMAIL_MAX_LEN: usize = 254;
pub const PASSWORD_MIN_CHARS: usize = 10;
/// Argon2 accepts more, this only stops absurdly long inputs.
pub const PASSWORD_MAX_CHARS: usize = 128;
pub const MAX_AGE_YEARS: u32 = 130;

const EMAIL_INDEX: &str = "users_email_lower_idx";

/// Columns loaded into `User`. A macro so it can be used in `concat!`,
/// sqlx only accepts SQL known at compile time.
/// `user_columns!("u.")` prefixes every column, for queries joining `users AS u`.
macro_rules! user_columns {
    () => {
        user_columns!("")
    };
    ($prefix:literal) => {
        concat!(
            $prefix, "id, ", $prefix, "username, ", $prefix, "displayname, ",
            $prefix, "firstname, ", $prefix, "lastname, ", $prefix, "birthday, ",
            $prefix, "gender, ", $prefix, "bio, ", $prefix, "names_visibility, ",
            $prefix, "birthday_public, ",
            $prefix, "photo_url, ", $prefix, "background_url, ", $prefix, "created_at, ",
            $prefix, "session_version, ", $prefix, "public_id, ",
            $prefix, "federation_enabled, ", $prefix, "role, ", $prefix, "suspended_at, ", $prefix, "suspended_until, ", $prefix, "team, ", $prefix, "search_indexing, ", $prefix, "indexable, ",
            $prefix, "account_kind, ", $prefix, "bot_contact, ", $prefix, "limited_at, ",
            $prefix, "email_verified"
        )
    };
}
pub(crate) use user_columns;

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "users",
    columns: &[
        Column::new("id", "SERIAL").primary_key(),
        // Random 16-digit id that can be shown publicly, unlike `id`.
        Column::new("public_id", "BIGINT")
            .default("1000000000000000 + (random() * 8999999999999999)::bigint"),
        Column::new("username", "TEXT"),
        Column::new("displayname", "TEXT").default("''"),
        Column::new("firstname", "TEXT").default("''"),
        Column::new("lastname", "TEXT").default("''"),
        // Nullable for accounts created before it was asked at signup.
        Column::new("birthday", "DATE").nullable(),
        Column::new("gender", "TEXT").default("''"),
        Column::new("bio", "TEXT").default("''"),
        // Privacy: who sees the first and last name, see `NameVisibility`.
        Column::new("names_visibility", "TEXT").default("'public'"),
        // Private by default (Law 25 art. 9.1: the most private settings
        // first); accounts made before keep theirs.
        Column::new("birthday_public", "BOOLEAN").default("FALSE"),
        // Consent to the Terms of Use and Privacy Policy: when, and which
        // versions (see `site_page::consent_version`).
        Column::new("terms_accepted_at", "TIMESTAMPTZ").nullable(),
        Column::new("terms_version", "TEXT").default("''"),
        // Search engines may show the profile and posts: off by default,
        // `noindex` on the pages otherwise.
        Column::new("search_indexing", "BOOLEAN").default("FALSE"),
        // Mastodon's `indexable`: the public posts may be found by the
        // search, here and on other servers. On unless turned off.
        Column::new("indexable", "BOOLEAN").default("TRUE"),
        // The language they last used (`en`, `fr`...), for the emails sent
        // without a request of theirs: moderation notices.
        Column::new("language", "TEXT").default("''"),
        // `person`, `bot` or `parody`, see `AccountKind` (the rules ask to
        // label them); a bot names the human answering for it.
        Column::new("account_kind", "TEXT").default("'person'"),
        Column::new("bot_contact", "TEXT").default("''"),
        // Limited by the moderators: only their followers see what they post.
        Column::new("limited_at", "TIMESTAMPTZ").nullable(),
        // `user`, `moderator` or `admin`, see `Role`. The first account made
        // is the admin, see `create_account`.
        Column::new("role", "TEXT").default("'user'"),
        // Nullable for accounts created before login existed, they can't sign in.
        Column::new("email", "TEXT").nullable(),
        Column::new("password_hash", "TEXT").nullable(),
        // Increased when the password changes: sessions signed in with an
        // older value are signed out, see `handlers::session::signed_in`.
        Column::new("session_version", "INTEGER").default("0"),
        Column::new("email_verified", "BOOLEAN").default("FALSE"),
        Column::new("email_verification_token", "TEXT").nullable(),
        Column::new("email_verification_sent_at", "TIMESTAMPTZ").nullable(),
        Column::new("email_verification_expires_at", "TIMESTAMPTZ").nullable(),
        Column::new("email_verified_at", "TIMESTAMPTZ").nullable(),
        Column::new("photo_url", "TEXT").nullable(),
        // CSS applied to the image, to crop or move it.
        Column::new("photo_style", "TEXT").default("''"),
        Column::new("background_url", "TEXT").nullable(),
        Column::new("background_style", "TEXT").default("''"),
        // The originals of the photo and background cropped in the browser,
        // to crop them again: never shown nor sent, see `image_originals`.
        Column::new("photo_original_url", "TEXT").nullable(),
        Column::new("background_original_url", "TEXT").nullable(),
        // ActivityPub actor. Nullable until the keys and URLs are generated.
        Column::new("ap_id", "TEXT").nullable(),
        Column::new("ap_inbox", "TEXT").nullable(),
        Column::new("ap_outbox", "TEXT").nullable(),
        Column::new("ap_public_key", "TEXT").nullable(),
        Column::new("ap_private_key", "TEXT").nullable(),
        // FALSE for users from other servers.
        Column::new("ap_local", "BOOLEAN").default("TRUE"),
        Column::new("ap_last_refreshed_at", "TIMESTAMPTZ").default("NOW()"),
        Column::new("federation_enabled", "BOOLEAN").default("TRUE"),
        Column::new("federation_display_name_mode", "TEXT").default("'full_name'"),
        // Set by the site's staff: can't sign in, posts and profile hidden.
        Column::new("suspended_at", "TIMESTAMPTZ").nullable(),
        // The account of the moderation team, see `sync_team_account`.
        Column::new("team", "BOOLEAN").default("FALSE"),
        // The end of a suspension; none with `suspended_at` is a ban, for good.
        Column::new("suspended_until", "TIMESTAMPTZ").nullable(),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[
        // Like Mastodon: case is kept, but "Alice" and "alice" can't both exist.
        Index::unique("users_username_lower_idx", "LOWER(username)"),
        Index::unique(EMAIL_INDEX, "LOWER(email)"),
        Index::unique("users_public_id_idx", "public_id"),
        // `search`: fast with ILIKE.
        Index::trigram("users_username_trgm_idx", "username"),
        Index::trigram("users_displayname_trgm_idx", "displayname"),
        Index::unique("users_email_verification_token_idx", "email_verification_token"),
        // Searching users by name.
        Index::new("users_displayname_lower_idx", "LOWER(displayname)"),
        Index::new("users_role_idx", "role"),
    ],
};

/// Who sees a user's first and last name. Their friends see what their
/// followers see. Elsewhere (feeds, search, other servers) only a public
/// name is shown: the display name or the username otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NameVisibility {
    #[default]
    Public,
    Followers,
    Friends,
    /// Only the user.
    Private,
}

impl NameVisibility {
    pub const ALL: [Self; 4] = [Self::Public, Self::Followers, Self::Friends, Self::Private];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Followers => "followers",
            Self::Friends => "friends",
            Self::Private => "private",
        }
    }

    /// Unknown values hide the name: a typo never shows it.
    pub fn parse(value: &str) -> Self {
        Self::ALL.into_iter().find(|visibility| visibility.as_str() == value).unwrap_or(Self::Private)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Public => "Everyone, here and on other servers",
            Self::Followers => "Your followers and friends here",
            Self::Friends => "Your friends here",
            Self::Private => "Only you",
        }
    }

    /// Shown next to the name for its owner.
    pub fn note(self) -> &'static str {
        match self {
            Self::Public => "",
            Self::Followers => "followers and friends",
            Self::Friends => "friends",
            Self::Private => "only you",
        }
    }

    /// The choices: friends only with `DFRN` on (keeping the current one).
    pub fn choices(current: Self) -> Vec<Self> {
        let dfrn = crate::settings::services().dfrn;
        Self::ALL.into_iter().filter(|choice| dfrn || *choice != Self::Friends || *choice == current).collect()
    }

    /// Whether a viewer sees the name: `follows` and `friends` say what they
    /// are to its owner.
    pub fn shown_to(self, own: bool, follows: bool, friends: bool) -> bool {
        own || match self {
            Self::Public => true,
            Self::Followers => follows || friends,
            Self::Friends => friends,
            Self::Private => false,
        }
    }
}

/// What a user can do on the site.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    User,
    /// Handles reports, see `handlers::moderation`.
    Moderator,
    /// A moderator who made the site: shown as its creator, and the only
    /// one naming moderators.
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Moderator => "moderator",
            Self::Admin => "admin",
        }
    }

    /// Unknown values are users: a typo never gives powers.
    pub fn parse(value: &str) -> Self {
        match value {
            "moderator" => Self::Moderator,
            "admin" => Self::Admin,
            _ => Self::User,
        }
    }

    /// Shown next to the name, `None` for users.
    pub fn badge(self) -> Option<&'static str> {
        match self {
            Self::User => None,
            Self::Moderator => Some("Moderator"),
            Self::Admin => Some("Creator"),
        }
    }
}

/// What an account is, shown as a badge by its name (Server rules, 7).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccountKind {
    #[default]
    Person,
    /// Posts by itself: a `Service` for other servers, Mastodon's "bot".
    Bot,
    /// A parody, fan or commentary account.
    Parody,
}

impl AccountKind {
    pub const ALL: [Self; 3] = [Self::Person, Self::Bot, Self::Parody];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Bot => "bot",
            Self::Parody => "parody",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "bot" => Self::Bot,
            "parody" => Self::Parody,
            _ => Self::Person,
        }
    }

    /// For the choice in the settings.
    pub fn label(self) -> &'static str {
        match self {
            Self::Person => "A person or an organization",
            Self::Bot => "A bot: it posts automatically",
            Self::Parody => "A parody, fan or commentary account",
        }
    }

    /// Shown next to the name, `None` for people.
    pub fn badge(self) -> Option<&'static str> {
        match self {
            Self::Person => None,
            Self::Bot => Some("Bot"),
            Self::Parody => Some("Parody"),
        }
    }

    pub fn badge_title(self) -> &'static str {
        match self {
            Self::Person => "",
            Self::Bot => "An automated account",
            Self::Parody => "A parody, fan or commentary account: not who it imitates",
        }
    }
}

/// Longest contact of a bot's human.
pub const BOT_CONTACT_MAX_CHARS: usize = 200;

#[derive(Clone, Debug)]
pub struct User {
    pub id: i32,
    /// Case is kept, but unique ignoring case, see `TABLE`.
    pub username: String,
    /// Name shown on the profile, can be empty.
    pub displayname: String,
    pub firstname: String,
    pub lastname: String,
    /// Asked at signup, `None` for accounts created before that.
    pub birthday: Option<NaiveDate>,
    pub gender: String,
    pub bio: String,
    /// Who sees `firstname` and `lastname`.
    pub names_visibility: NameVisibility,
    /// Whether other users can see it on the profile.
    pub birthday_public: bool,
    pub photo_url: Option<String>,
    pub background_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub session_version: i32,
    /// Never changes, unlike the username: used in Fediverse addresses.
    pub public_id: i64,
    /// Whether other Fediverse servers can find and follow the account.
    pub federation_enabled: bool,
    pub role: Role,
    /// Banned or suspended by the staff, see `restrict`.
    pub suspended_at: Option<DateTime<Utc>>,
    /// The end of a suspension, `None` for a ban.
    pub suspended_until: Option<DateTime<Utc>>,
    /// The account of the moderation team, see `sync_team_account`.
    pub team: bool,
    /// Search engines may show the profile and posts (`noindex` otherwise).
    pub search_indexing: bool,
    /// Mastodon's `indexable`: the public posts may be found by the search.
    pub indexable: bool,
    /// A person, a bot or a parody, see `AccountKind`.
    pub account_kind: AccountKind,
    /// For a bot: who answers for it (`@alice` or an email).
    pub bot_contact: String,
    /// Limited by the moderators, see `set_limited`.
    pub limited_at: Option<DateTime<Utc>>,
    /// The link sent to the email was opened, see `handlers::verify_email`.
    pub email_verified: bool,
}

impl User {
    pub fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            displayname: row.try_get("displayname")?,
            firstname: row.try_get("firstname")?,
            lastname: row.try_get("lastname")?,
            birthday: row.try_get("birthday")?,
            gender: row.try_get("gender")?,
            bio: row.try_get("bio")?,
            names_visibility: NameVisibility::parse(&row.try_get::<String, _>("names_visibility")?),
            birthday_public: row.try_get("birthday_public")?,
            photo_url: row.try_get("photo_url")?,
            background_url: row.try_get("background_url")?,
            created_at: row.try_get("created_at")?,
            session_version: row.try_get("session_version")?,
            public_id: row.try_get("public_id")?,
            federation_enabled: row.try_get("federation_enabled")?,
            role: Role::parse(&row.try_get::<String, _>("role")?),
            suspended_at: row.try_get("suspended_at")?,
            suspended_until: row.try_get("suspended_until")?,
            team: row.try_get("team")?,
            search_indexing: row.try_get("search_indexing")?,
            indexable: row.try_get("indexable")?,
            account_kind: AccountKind::parse(&row.try_get::<String, _>("account_kind")?),
            bot_contact: row.try_get("bot_contact")?,
            limited_at: row.try_get("limited_at")?,
            email_verified: row.try_get("email_verified")?,
        })
    }

    /// "First Last", whoever may see it: empty when neither is written.
    pub fn real_name(&self) -> String {
        [self.firstname.as_str(), self.lastname.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The real name when everyone may see it, else empty.
    pub fn full_name(&self) -> String {
        match self.names_visibility {
            NameVisibility::Public => self.real_name(),
            _ => String::new(),
        }
    }

    /// The choices of who sees the names, for the settings.
    pub fn name_visibility_choices(&self) -> Vec<NameVisibility> {
        NameVisibility::choices(self.names_visibility)
    }

    /// The display name, else the public full name, else the username.
    /// Shown to everyone and sent to other servers (Mastodon, Hubzilla...),
    /// so it never contains a name that isn't public.
    pub fn display_name(&self) -> String {
        if !self.displayname.is_empty() {
            return self.displayname.clone();
        }
        let full_name = self.full_name();
        if full_name.is_empty() {
            self.username.clone()
        } else {
            full_name
        }
    }

    /// First letter of the display name, shown when there is no photo.
    pub fn initial(&self) -> String {
        self.display_name()
            .chars()
            .next()
            .map(|letter| letter.to_uppercase().collect())
            .unwrap_or_default()
    }

    /// Site moderators and admins: they can ban or delete any community.
    /// Moderators and the admin: they handle reports.
    pub fn is_staff(&self) -> bool {
        matches!(self.role, Role::Moderator | Role::Admin)
    }

    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// Banned, or suspended and not yet at its end.
    pub fn is_suspended(&self) -> bool {
        self.suspended_at.is_some() && self.suspended_until.is_none_or(|until| until > Utc::now())
    }

    /// The end of a suspension, for the person; `None` for a ban.
    pub fn suspension_end_label(&self) -> Option<String> {
        self.suspended_until.map(crate::i18n::date_time)
    }

    pub fn is_bot(&self) -> bool {
        self.account_kind == AccountKind::Bot
    }

    pub fn is_limited(&self) -> bool {
        self.limited_at.is_some()
    }

    /// `@alice@tison.social`: the username with the site's domain.
    pub fn handle(&self) -> String {
        format!("@{}@{}", self.username, crate::settings::site_domain())
    }

    /// Restricted for good, not until a date.
    pub fn is_banned(&self) -> bool {
        self.suspended_at.is_some() && self.suspended_until.is_none()
    }

    /// Other Fediverse servers can see and reach the account: on the
    /// Fediverse, and not suspended.
    pub fn is_federated(&self) -> bool {
        self.federation_enabled && !self.is_suspended()
    }

    pub fn image_url(&self, image: ProfileImage) -> Option<&str> {
        match image {
            ProfileImage::Photo => self.photo_url.as_deref(),
            ProfileImage::Background => self.background_url.as_deref(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ProfileImage {
    Photo,
    Background,
}

impl ProfileImage {
    /// Used in uploaded file names.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Background => "background",
        }
    }
}

/// Checks the username only uses `A-Z`, `a-z`, `0-9` and `_`. Case is kept.
pub fn validate_username(raw: &str) -> Option<String> {
    let username = raw.trim();
    let valid_length = (USERNAME_MIN_LEN..=USERNAME_MAX_LEN).contains(&username.len());
    let valid_chars = username
        .bytes()
        .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric());
    (valid_length && valid_chars).then(|| username.to_string())
}

/// Trims the bio, keeps its line breaks, and rejects other control characters
/// or bios that are too long. An empty bio is valid: it removes the bio.
pub fn validate_bio(raw: &str) -> Option<String> {
    // Browsers send line breaks as \r\n.
    let bio = raw.trim().replace("\r\n", "\n");
    let valid = bio.chars().count() <= BIO_MAX_CHARS
        && !bio
            .chars()
            .any(|letter| letter.is_control() && !matches!(letter, '\n' | '\t'));
    valid.then_some(bio)
}

/// Trims the name and rejects control characters or names that are too long.
/// An empty name is valid: the username is displayed instead.
pub fn validate_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    let valid = name.chars().count() <= NAME_MAX_CHARS && !name.chars().any(char::is_control);
    valid.then(|| name.to_string())
}

/// Lowercases the email and checks it looks like `name@domain.tld`.
/// Only a basic check: the address is not verified to exist.
pub fn validate_email(raw: &str) -> Option<String> {
    let email = raw.trim().to_lowercase();
    let (local, domain) = email.split_once('@')?;
    let valid = email.len() <= EMAIL_MAX_LEN
        && !local.is_empty()
        && !domain.contains('@')
        && !email.chars().any(char::is_whitespace)
        && domain
            .split_once('.')
            .is_some_and(|(name, _)| !name.is_empty())
        && !domain.ends_with('.')
        && !domain.contains("..");
    valid.then_some(email)
}

/// Same rules as the old Instavox signup: at least `PASSWORD_MIN_CHARS` characters,
/// with an uppercase letter, a lowercase letter, a digit and a symbol.
/// Spaces are allowed only one at a time, between words.
pub fn validate_password(password: &str) -> bool {
    let length = password.chars().count();
    if !(PASSWORD_MIN_CHARS..=PASSWORD_MAX_CHARS).contains(&length)
        || password.starts_with(' ')
        || password.ends_with(' ')
        || password.contains("  ")
    {
        return false;
    }

    let (mut upper, mut lower, mut digit, mut symbol) = (false, false, false, false);
    for letter in password.chars() {
        match letter {
            ' ' => {}
            _ if letter.is_whitespace() || letter.is_control() => return false,
            _ if letter.is_uppercase() => upper = true,
            _ if letter.is_lowercase() => lower = true,
            _ if letter.is_ascii_digit() => digit = true,
            _ => symbol = true,
        }
    }
    upper && lower && digit && symbol
}

/// Parses a `YYYY-MM-DD` date (what `<input type="date">` sends) and checks
/// the user is at least `min_age` old (`settings::minimum_age`) and the date
/// is plausible.
pub fn validate_birthday(raw: &str, today: NaiveDate, min_age: u32) -> Option<NaiveDate> {
    let birthday = NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").ok()?;
    let age = today.years_since(birthday)?;
    (min_age..=MAX_AGE_YEARS)
        .contains(&age)
        .then_some(birthday)
}

/// Fields of a new account, already validated.
pub struct NewAccount {
    pub username: String,
    pub email: String,
    pub password_hash: String,
    /// Both optional: the username is enough.
    pub firstname: String,
    pub lastname: String,
    pub names_visibility: NameVisibility,
    pub birthday: NaiveDate,
    /// The versions of the Terms and Privacy Policy accepted at signup.
    pub terms_version: String,
}

#[derive(Debug)]
pub enum CreateAccountError {
    UsernameTaken,
    EmailTaken,
    Database(sqlx::Error),
}

pub async fn create_account(pool: &PgPool, account: &NewAccount) -> Result<User, CreateAccountError> {
    // Not a community's name either: they share the Fediverse names. The
    // first account of the site is its admin, whoever installed it.
    let result = sqlx::query(concat!(
        "INSERT INTO users (username, email, password_hash, firstname, lastname, birthday, role, names_visibility,
                            terms_accepted_at, terms_version)
         SELECT $1, $2, $3, $4, $5, $6, CASE WHEN EXISTS (SELECT 1 FROM users WHERE NOT team) THEN 'user' ELSE 'admin' END, $7,
                NOW(), $8
         WHERE NOT EXISTS (SELECT 1 FROM communities WHERE LOWER(slug) = LOWER($1))
         RETURNING ",
        user_columns!()
    ))
    .bind(&account.username)
    .bind(&account.email)
    .bind(&account.password_hash)
    .bind(&account.firstname)
    .bind(&account.lastname)
    .bind(account.birthday)
    .bind(account.names_visibility.as_str())
    .bind(&account.terms_version)
    .fetch_optional(pool)
    .await;

    match result {
        Ok(Some(row)) => {
            let user = User::from_row(&row).map_err(CreateAccountError::Database)?;
            follow_team(pool, Some(user.id)).await.map_err(CreateAccountError::Database)?;
            Ok(user)
        }
        Ok(None) => Err(CreateAccountError::UsernameTaken),
        Err(err) if is_unique_violation(&err) => {
            let constraint = err.as_database_error().and_then(|db_err| db_err.constraint());
            if constraint == Some(EMAIL_INDEX) {
                Err(CreateAccountError::EmailTaken)
            } else {
                Err(CreateAccountError::UsernameTaken)
            }
        }
        Err(err) => Err(CreateAccountError::Database(err)),
    }
}

/// Id and password hash of the account matching a username or an email, ignoring case.
/// The hash is `None` for accounts created before passwords existed.
pub async fn find_credentials(
    pool: &PgPool,
    login: &str,
) -> Result<Option<(i32, Option<String>)>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, password_hash FROM users
         WHERE LOWER(username) = LOWER($1) OR LOWER(email) = LOWER($1)
         LIMIT 1",
    )
    .bind(login.trim())
    .fetch_optional(pool)
    .await?;

    row.map(|row| Ok((row.try_get("id")?, row.try_get("password_hash")?)))
        .transpose()
}

pub async fn find_by_id(pool: &PgPool, id: i32) -> Result<Option<User>, sqlx::Error> {
    sqlx::query(concat!("SELECT ", user_columns!(), " FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(User::from_row)
        .transpose()
}

pub async fn find_by_public_id(pool: &PgPool, public_id: i64) -> Result<Option<User>, sqlx::Error> {
    sqlx::query(concat!("SELECT ", user_columns!(), " FROM users WHERE public_id = $1"))
        .bind(public_id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(User::from_row)
        .transpose()
}

pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query(concat!(
        "SELECT ",
        user_columns!(),
        " FROM users WHERE LOWER(username) = LOWER($1)"
    ))
    .bind(username.trim())
    .fetch_optional(pool)
    .await?
    .as_ref()
    .map(User::from_row)
    .transpose()
}

/// Gives a role to a user.
pub async fn set_role(pool: &PgPool, id: i32, role: Role) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET role = $2 WHERE id = $1")
        .bind(id)
        .bind(role.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

/// The site's staff: the admin first, then the moderators by name.
pub async fn staff(pool: &PgPool) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query(concat!(
        "SELECT ",
        user_columns!(),
        " FROM users WHERE role IN ('admin', 'moderator')
         ORDER BY role = 'admin' DESC, LOWER(username)"
    ))
    .fetch_all(pool)
    .await?
    .iter()
    .map(User::from_row)
    .collect()
}

/// Bans an account (`days` none) or suspends it for some days. It's also
/// signed out everywhere (`session_version`).
pub async fn restrict(pool: &PgPool, id: i32, days: Option<i32>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE users SET suspended_at = NOW(), suspended_until = NOW() + make_interval(days => $2),
                          session_version = session_version + 1
         WHERE id = $1",
    )
    .bind(id)
    .bind(days)
    .execute(pool)
    .await?;
    Ok(())
}

/// `Tison Social` → `tison_social`: the start of the team account's names.
fn team_slug(instance_name: &str) -> String {
    let mut slug = String::new();
    for letter in instance_name.trim().chars().flat_map(char::to_lowercase) {
        if letter.is_ascii_alphanumeric() {
            slug.push(letter);
        } else if !slug.is_empty() && !slug.ends_with('_') {
            slug.push('_');
        }
    }
    // With `_team`, within `USERNAME_MAX_LEN`.
    slug.truncate(USERNAME_MAX_LEN - "_team".len());
    let slug = slug.trim_end_matches('_');
    if slug.is_empty() { "tison".to_string() } else { slug.to_string() }
}

/// The username and email of the moderation team's account: `tison_team`
/// (usernames have no `-`) and `tison-team@local.invalid`, an address that
/// can't receive emails.
pub fn team_names(instance_name: &str) -> (String, String) {
    let slug = team_slug(instance_name);
    (format!("{slug}_team"), format!("{}-team@local.invalid", slug.replace('_', "-")))
}

/// The account of the moderation team (`MULTI-MODERATOR` in settings.conf):
/// a moderator nobody signs in to (no password), shown as "{name} Team".
/// Made when it's turned on, hidden like a ban when it's off: its posts stay.
pub async fn sync_team_account(pool: &PgPool, instance_name: &str, active: bool) -> Result<(), sqlx::Error> {
    let existing: Option<i32> = sqlx::query_scalar("SELECT id FROM users WHERE team").fetch_optional(pool).await?;
    match existing {
        Some(id) => {
            sqlx::query(
                "UPDATE users SET suspended_at = CASE WHEN $2 THEN NULL ELSE COALESCE(suspended_at, NOW()) END,
                                  suspended_until = NULL, role = 'moderator'
                 WHERE id = $1",
            )
            .bind(id)
            .bind(active)
            .execute(pool)
            .await?;
        }
        None if active => {
            let (username, email) = team_names(instance_name);
            let created = sqlx::query(
                "INSERT INTO users (username, email, displayname, role, team, email_verified, bio)
                 SELECT $1, $2, $3, 'moderator', TRUE, TRUE, $4
                 WHERE NOT EXISTS (SELECT 1 FROM users WHERE LOWER(username) = LOWER($1) OR LOWER(email) = LOWER($2))
                   AND NOT EXISTS (SELECT 1 FROM communities WHERE LOWER(slug) = LOWER($1))",
            )
            .bind(&username)
            .bind(&email)
            .bind(format!("{} Team", instance_name.trim()))
            .bind(format!("The moderation team of {}.", instance_name.trim()))
            .execute(pool)
            .await?;
            if created.rows_affected() == 0 {
                tracing::warn!("the team account @{} can't be made: the name is taken", username);
            } else {
                // Everyone follows it, like the ones signing up later.
                follow_team(pool, None).await?;
                tracing::info!("team account @{} ({}) made", username, email);
            }
        }
        None => {}
    }
    Ok(())
}

/// The versions of the Terms and Privacy Policy the user accepted last,
/// and when.
pub async fn consent(pool: &PgPool, id: i32) -> Result<(String, Option<DateTime<Utc>>), sqlx::Error> {
    Ok(sqlx::query_as("SELECT terms_version, terms_accepted_at FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .unwrap_or_default())
}

/// The user accepts the Terms and Privacy Policy of `version`.
pub async fn accept_terms(pool: &PgPool, id: i32, version: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET terms_accepted_at = NOW(), terms_version = $2 WHERE id = $1").bind(id).bind(version).execute(pool).await?;
    Ok(())
}

/// The language of the user's emails, see the `language` column.
pub async fn set_language(pool: &PgPool, id: i32, code: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET language = $2 WHERE id = $1 AND language <> $2").bind(id).bind(code).execute(pool).await?;
    Ok(())
}

/// The email and language of a user, for the emails sent to them.
pub async fn email_and_language(pool: &PgPool, id: i32) -> Result<Option<(String, String)>, sqlx::Error> {
    let row: Option<(Option<String>, String)> = sqlx::query_as("SELECT email, language FROM users WHERE id = $1").bind(id).fetch_optional(pool).await?;
    Ok(row.and_then(|(email, language)| email.map(|email| (email, language))))
}

/// A person, a bot (with the human answering for it) or a parody.
pub async fn set_account_kind(pool: &PgPool, id: i32, kind: AccountKind, bot_contact: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET account_kind = $2, bot_contact = $3 WHERE id = $1")
        .bind(id)
        .bind(kind.as_str())
        .bind(bot_contact)
        .execute(pool)
        .await?;
    Ok(())
}

/// Whether the bot `bot` may comment on the post `post_id` (answering its
/// comment `parent_id`, if given): only someone who interacts with it
/// (Server rules, 7): on its own post (commenting there is talking to it),
/// to its own comment, someone following it, or a text mentioning it.
pub async fn bot_may_reply(pool: &PgPool, bot: &User, post_id: i64, parent_id: Option<i64>) -> Result<bool, sqlx::Error> {
    let row: Option<(Option<i32>, Option<i32>, Option<i64>, String)> = sqlx::query_as(
        "SELECT p.user_id, COALESCE(c.user_id, CASE WHEN $2::bigint IS NULL THEN p.user_id END),
                COALESCE(c.remote_actor_id, CASE WHEN $2::bigint IS NULL THEN p.remote_actor_id END),
                COALESCE(c.body, CASE WHEN $2::bigint IS NULL THEN COALESCE(p.title, '') || ' ' || p.body END, '')
         FROM posts p LEFT JOIN comments c ON c.id = $2 AND c.post_id = p.id
         WHERE p.id = $1",
    )
    .bind(post_id)
    .bind(parent_id)
    .fetch_optional(pool)
    .await?;
    let Some((post_author, user_id, actor_id, text)) = row else {
        return Ok(false);
    };
    if post_author == Some(bot.id) || user_id == Some(bot.id) || mentions(&text, &bot.username) {
        return Ok(true);
    }
    follows_bot(pool, bot.id, user_id, actor_id).await
}

/// Whether the bot may write to this person in the messenger: someone
/// following it, or who already wrote to it there (Server rules, 7).
pub async fn bot_may_message(pool: &PgPool, bot_id: i32, conversation_id: i64, user_id: Option<i32>, actor_id: Option<i64>) -> Result<bool, sqlx::Error> {
    let wrote: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM messages WHERE conversation_id = $1
            AND (sender_user_id = $2 OR sender_actor_id = $3))",
    )
    .bind(conversation_id)
    .bind(user_id)
    .bind(actor_id)
    .fetch_one(pool)
    .await?;
    Ok(wrote || follows_bot(pool, bot_id, user_id, actor_id).await?)
}

async fn follows_bot(pool: &PgPool, bot_id: i32, user_id: Option<i32>, actor_id: Option<i64>) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_follows WHERE followed_id = $1 AND follower_id = $2)
             OR EXISTS (SELECT 1 FROM federation_followers WHERE user_id = $1 AND actor_id = $3)",
    )
    .bind(bot_id)
    .bind(user_id)
    .bind(actor_id)
    .fetch_one(pool)
    .await
}

/// `@name` in `text`, alone (not `@namesake`), any case, with or without
/// its server (`@name@tison.social`).
pub fn mentions(text: &str, username: &str) -> bool {
    let lower = text.to_lowercase();
    let wanted = format!("@{}", username.to_lowercase());
    lower.match_indices(&wanted).any(|(at, _)| {
        let before_ok = lower[..at].chars().next_back().is_none_or(|letter| !(letter.is_alphanumeric() || letter == '_'));
        let after_ok = lower[at + wanted.len()..].chars().next().is_none_or(|letter| !(letter.is_alphanumeric() || letter == '_'));
        before_ok && after_ok
    })
}

/// Public posts of the user in the last hour, for the bots' limit.
pub async fn posts_last_hour(pool: &PgPool, id: i32) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM posts WHERE user_id = $1 AND visibility <> 'private' AND created_at > NOW() - INTERVAL '1 hour'")
        .bind(id)
        .fetch_one(pool)
        .await
}

/// Limits the account (Server rules, enforcement): only their followers,
/// friends and the staff see what they post, see `post_visible!`.
pub async fn set_limited(pool: &PgPool, id: i32, limited: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET limited_at = CASE WHEN $2 THEN COALESCE(limited_at, NOW()) END WHERE id = $1")
        .bind(id)
        .bind(limited)
        .execute(pool)
        .await?;
    Ok(())
}

/// Whether the public posts of the user may be found by the search.
pub async fn set_indexable(pool: &PgPool, id: i32, indexable: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET indexable = $2 WHERE id = $1").bind(id).bind(indexable).execute(pool).await?;
    Ok(())
}

pub async fn set_search_indexing(pool: &PgPool, id: i32, allowed: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET search_indexing = $2 WHERE id = $1").bind(id).bind(allowed).execute(pool).await?;
    Ok(())
}

/// The team account when it's on (`MULTI-MODERATOR`), else `None`.
pub async fn team_account(pool: &PgPool) -> Result<Option<User>, sqlx::Error> {
    sqlx::query(concat!("SELECT ", user_columns!(), " FROM users WHERE team AND suspended_at IS NULL"))
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(User::from_row)
        .transpose()
}

/// `user_id` (or every user) follows the team account when it's on. They
/// can unfollow it like any account.
async fn follow_team(pool: &PgPool, user_id: Option<i32>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_follows (follower_id, followed_id)
         SELECT u.id, t.id FROM users u, users t
         WHERE t.team AND t.suspended_at IS NULL AND NOT u.team AND ($1::INTEGER IS NULL OR u.id = $1)
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Lifts a ban or a suspension.
pub async fn reinstate(pool: &PgPool, id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET suspended_at = NULL, suspended_until = NULL WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Ends the suspensions whose time is over: the queries hiding suspended
/// accounts only look at `suspended_at`. Returns the accounts.
pub async fn end_suspensions(pool: &PgPool) -> Result<Vec<i32>, sqlx::Error> {
    sqlx::query_scalar("UPDATE users SET suspended_at = NULL, suspended_until = NULL WHERE suspended_until <= NOW() RETURNING id")
        .fetch_all(pool)
        .await
}

/// Deletes an account and all it made (posts, comments, messages...): the
/// tables referencing users cascade, reports keep their snapshot.
pub async fn delete(pool: &PgPool, id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM users WHERE id = $1").bind(id).execute(pool).await?;
    Ok(())
}

/// An account in the staff's list of users (Moderation > Users).
pub struct ListedUser {
    pub user: User,
    /// `None` for accounts made before emails were asked.
    pub email: Option<String>,
}

impl ListedUser {
    /// "Active", "Banned", "Suspended until Nov 9, 2026".
    pub fn status(&self) -> String {
        match (self.user.suspended_at, self.user.suspended_until) {
            (None, _) => crate::i18n::t("Active"),
            (Some(_), None) => crate::i18n::t("Banned"),
            (Some(_), Some(until)) => crate::i18n::tf("Suspended until {}", crate::i18n::date(until.date_naive())),
        }
    }

    pub fn joined_label(&self) -> String {
        crate::i18n::date(self.user.created_at.date_naive())
    }
}

/// Users shown at once in the staff's list.
pub const LIST_SIZE: i64 = 50;

/// The site's accounts for the staff, newest first, older than the id
/// `before`, those whose username, display name or email contains `search`.
pub async fn list_for_staff(pool: &PgPool, search: &str, before: Option<i32>) -> Result<Vec<ListedUser>, sqlx::Error> {
    let search = escape_like(&search.trim().trim_start_matches('@').to_lowercase());
    sqlx::query(concat!(
        "SELECT ",
        user_columns!(),
        ", email FROM users
         WHERE ($1 = '' OR LOWER(username) LIKE '%' || $1 || '%' OR LOWER(displayname) LIKE '%' || $1 || '%'
                OR LOWER(COALESCE(email, '')) LIKE '%' || $1 || '%')
           AND ($2::INTEGER IS NULL OR id < $2)
         ORDER BY id DESC LIMIT $3"
    ))
    .bind(search)
    .bind(before)
    .bind(LIST_SIZE)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| Ok(ListedUser { user: User::from_row(row)?, email: row.try_get("email")? }))
    .collect()
}


/// `%` and `_` typed by the user are searched as is in a LIKE pattern.
pub fn escape_like(text: &str) -> String {
    text.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Users whose username, display name, or public first or last name contains
/// `search`, ignoring case. Most relevant first: the exact username, then
/// names starting with it, then names with a word starting with it.
pub async fn search(pool: &PgPool, search: &str, limit: i64) -> Result<Vec<User>, sqlx::Error> {
    let search = search.trim().trim_start_matches('@').to_lowercase();
    let escaped = escape_like(&search);
    sqlx::query(concat!(
        "SELECT ",
        user_columns!(),
        " FROM users,
              LATERAL (SELECT LOWER(displayname) AS display,
                              CASE WHEN names_visibility = 'public' THEN LOWER(firstname) ELSE '' END AS first,
                              CASE WHEN names_visibility = 'public' THEN LOWER(lastname) ELSE '' END AS last) n
         WHERE suspended_at IS NULL
           AND (username ILIKE '%' || $1 || '%' OR displayname ILIKE '%' || $1 || '%'
            OR (names_visibility = 'public' AND (firstname ILIKE '%' || $1 || '%' OR lastname ILIKE '%' || $1 || '%')))
         ORDER BY CASE
                    WHEN LOWER(username) = $2 THEN 0
                    WHEN LOWER(username) LIKE $1 || '%' THEN 1
                    WHEN n.display LIKE $1 || '%' OR n.first LIKE $1 || '%' OR n.last LIKE $1 || '%' THEN 2
                    WHEN n.display LIKE '% ' || $1 || '%' THEN 3
                    ELSE 4
                  END,
                  LENGTH(username), LOWER(username)
         LIMIT $3"
    ))
    .bind(escaped)
    .bind(search)
    .bind(limit)
    .fetch_all(pool)
    .await?
    .iter()
    .map(User::from_row)
    .collect()
}

pub async fn update_names(
    pool: &PgPool,
    id: i32,
    displayname: &str,
    firstname: &str,
    lastname: &str,
    visibility: NameVisibility,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET displayname = $1, firstname = $2, lastname = $3, names_visibility = $5 WHERE id = $4")
        .bind(displayname)
        .bind(firstname)
        .bind(lastname)
        .bind(id)
        .bind(visibility.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

/// Hours a link to verify the email works.
pub const VERIFICATION_VALID_HOURS: i32 = 48;
/// Minimum time between two verification emails, against spam.
pub const VERIFICATION_RESEND_SECONDS: i32 = 120;

/// The email of the user, and whether it's verified.
pub async fn email_status(pool: &PgPool, id: i32) -> Result<Option<(String, bool)>, sqlx::Error> {
    let row = sqlx::query("SELECT email, email_verified FROM users WHERE id = $1 AND email IS NOT NULL")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.map(|row| Ok((row.try_get("email")?, row.try_get("email_verified")?))).transpose()
}

/// Stores the hash of a new verification link, replacing the one before:
/// `false` when one was sent less than `VERIFICATION_RESEND_SECONDS` ago, or
/// the email is already verified.
pub async fn start_verification(pool: &PgPool, id: i32, token_hash: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE users SET email_verification_token = $2, email_verification_sent_at = NOW(),
                email_verification_expires_at = NOW() + make_interval(hours => $3)
         WHERE id = $1 AND NOT email_verified AND email IS NOT NULL
           AND (email_verification_sent_at IS NULL OR email_verification_sent_at < NOW() - make_interval(secs => $4))",
    )
    .bind(id)
    .bind(token_hash)
    .bind(VERIFICATION_VALID_HOURS)
    .bind(f64::from(VERIFICATION_RESEND_SECONDS))
    .execute(pool)
    .await?
    .rows_affected()
        > 0)
}

/// Verifies the email whose link has this hash, if it hasn't expired: the
/// link then stops working. Returns the user.
pub async fn verify_email(pool: &PgPool, token_hash: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query(concat!(
        "UPDATE users SET email_verified = TRUE, email_verified_at = NOW(),
                email_verification_token = NULL, email_verification_expires_at = NULL
         WHERE email_verification_token = $1 AND email_verification_expires_at > NOW()
         RETURNING ",
        user_columns!()
    ))
    .bind(token_hash)
    .fetch_optional(pool)
    .await?
    .as_ref()
    .map(User::from_row)
    .transpose()
}

/// `None` for accounts created before passwords existed.
pub async fn password_hash(pool: &PgPool, id: i32) -> Result<Option<String>, sqlx::Error> {
    Ok(sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .flatten())
}

/// Saves the new password and signs out every session of the user
/// (they keep the old `session_version`). Returns the updated user.
pub async fn update_password(pool: &PgPool, id: i32, password_hash: &str) -> Result<User, sqlx::Error> {
    let row = sqlx::query(concat!(
        "UPDATE users SET password_hash = $1, session_version = session_version + 1
         WHERE id = $2 RETURNING ",
        user_columns!()
    ))
    .bind(password_hash)
    .bind(id)
    .fetch_one(pool)
    .await?;
    User::from_row(&row)
}

/// Id and stored email of the account using `email`, ignoring case.
pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<(i32, String)>, sqlx::Error> {
    let row = sqlx::query("SELECT id, email FROM users WHERE LOWER(email) = LOWER($1)")
        .bind(email)
        .fetch_optional(pool)
        .await?;
    row.map(|row| Ok((row.try_get("id")?, row.try_get("email")?)))
        .transpose()
}

pub async fn update_bio(pool: &PgPool, id: i32, bio: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET bio = $1 WHERE id = $2")
        .bind(bio)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Returns `false` when the username is already used by someone else, or by
/// a community, ignoring case.
pub async fn update_username(pool: &PgPool, id: i32, username: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE users SET username = $1 WHERE id = $2
         AND NOT EXISTS (SELECT 1 FROM communities WHERE LOWER(slug) = LOWER($1))",
    )
    .bind(username)
    .bind(id)
    .execute(pool)
    .await;

    match result {
        Ok(done) => Ok(done.rows_affected() > 0),
        Err(err) if is_unique_violation(&err) => Ok(false),
        Err(err) => Err(err),
    }
}

/// `None` removes the image.
/// The picture and the original it was cropped from (`None`: it's its own).
pub async fn update_image(
    pool: &PgPool,
    id: i32,
    image: ProfileImage,
    url: Option<&str>,
    original_url: Option<&str>,
) -> Result<(), sqlx::Error> {
    let query = match image {
        ProfileImage::Photo => "UPDATE users SET photo_url = $1, photo_original_url = $3 WHERE id = $2",
        ProfileImage::Background => "UPDATE users SET background_url = $1, background_original_url = $3 WHERE id = $2",
    };
    sqlx::query(query).bind(url).bind(id).bind(original_url).execute(pool).await?;
    Ok(())
}

/// The originals of the photo and the background, when they were cropped.
pub async fn image_originals(pool: &PgPool, id: i32) -> Result<(Option<String>, Option<String>), sqlx::Error> {
    Ok(sqlx::query_as("SELECT photo_original_url, background_original_url FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .unwrap_or_default())
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    err.as_database_error()
        .and_then(|db_err| db_err.code())
        .is_some_and(|code| code == "23505")
}

#[cfg(test)]
mod tests {
    #[test]
    fn mentions_are_whole_names() {
        assert!(mentions("hi @Weatherbot, rain?", "weatherbot"));
        assert!(mentions("@weatherbot@tison.social today", "weatherbot"));
        assert!(!mentions("hi @weatherbots", "weatherbot"));
        assert!(!mentions("mail me at x@weatherbot", "weatherbot"));
        assert!(!mentions("nothing", "weatherbot"));
    }

    #[test]
    fn account_kinds_parse_and_badge() {
        for kind in AccountKind::ALL {
            assert_eq!(AccountKind::parse(kind.as_str()), kind);
        }
        assert_eq!(AccountKind::parse("robot"), AccountKind::Person);
        assert_eq!(AccountKind::Bot.badge(), Some("Bot"));
        assert_eq!(AccountKind::Person.badge(), None);
    }

    use super::*;

    #[test]
    fn team_account_names() {
        assert_eq!(team_names("Tison"), ("tison_team".to_string(), "tison-team@local.invalid".to_string()));
        assert_eq!(team_names(" Lynx Social! "), ("lynx_social_team".to_string(), "lynx-social-team@local.invalid".to_string()));
        assert_eq!(team_names("???").0, "tison_team");
        assert!(validate_username(&team_names(&"Very long name ".repeat(5)).0).is_some());
    }

    fn named(displayname: &str, visibility: NameVisibility) -> User {
        User {
            id: 1,
            username: "ada".to_string(),
            displayname: displayname.to_string(),
            firstname: "Ada".to_string(),
            lastname: "Lovelace".to_string(),
            birthday: None,
            gender: String::new(),
            bio: String::new(),
            names_visibility: visibility,
            birthday_public: true,
            photo_url: None,
            background_url: None,
            created_at: Utc::now(),
            session_version: 0,
            public_id: 1,
            federation_enabled: true,
            role: Role::User,
            suspended_at: None,
            suspended_until: None,
            team: false,
            search_indexing: false,
            indexable: true,
            account_kind: AccountKind::Person,
            bot_contact: String::new(),
            limited_at: None,
            email_verified: true,
        }
    }

    #[test]
    fn names_are_shown_only_to_whom_they_may_be() {
        use NameVisibility::*;
        // (own, follows, friends)
        let viewers = [(true, false, false), (false, false, false), (false, true, false), (false, false, true)];
        let shown = |visibility: NameVisibility| viewers.map(|(own, follows, friends)| visibility.shown_to(own, follows, friends));
        assert_eq!(shown(Public), [true, true, true, true]);
        assert_eq!(shown(Followers), [true, false, true, true]);
        assert_eq!(shown(Friends), [true, false, false, true]);
        assert_eq!(shown(Private), [true, false, false, false]);
        assert_eq!(NameVisibility::parse("nonsense"), Private);
    }

    #[test]
    fn feeds_and_other_servers_only_get_a_public_name() {
        assert_eq!(named("", NameVisibility::Public).display_name(), "Ada Lovelace");
        assert_eq!(named("", NameVisibility::Followers).display_name(), "ada");
        assert_eq!(named("", NameVisibility::Private).display_name(), "ada");
        assert_eq!(named("Countess", NameVisibility::Friends).display_name(), "Countess");
        let mut nameless = named("", NameVisibility::Public);
        nameless.firstname.clear();
        nameless.lastname.clear();
        assert_eq!(nameless.display_name(), "ada");
    }

    #[test]
    fn roles_are_read_safely() {
        assert_eq!(Role::parse("admin"), Role::Admin);
        assert_eq!(Role::parse("moderator"), Role::Moderator);
        // A typo or an old value never gives powers.
        assert_eq!(Role::parse("Admin"), Role::User);
        assert_eq!(Role::parse("superuser"), Role::User);
        assert_eq!(Role::Admin.badge(), Some("Creator"));
        assert_eq!(Role::User.badge(), None);
    }
}
