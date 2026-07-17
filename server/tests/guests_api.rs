mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use common::{app, seed_user, send};

/// A three-part wedding, which is the shape this product exists for.
async fn seed_wedding(app: &axum::Router, session: &str) -> (Uuid, Vec<Uuid>) {
    let (status, body, _) = send(
        app,
        Method::POST,
        "/api/events",
        Some(json!({
            "title": "Tolu & Emeka",
            "timezone": "Africa/Lagos",
            "subEvents": [
                { "name": "Traditional Engagement", "startsAt": "2026-11-20T10:00:00Z" },
                { "name": "Church Ceremony", "startsAt": "2026-11-21T09:00:00Z" },
                { "name": "Reception", "startsAt": "2026-11-21T13:00:00Z" }
            ]
        })),
        Some(session),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let event_id = body["id"].as_str().unwrap().parse().unwrap();
    let part_ids = body["subEvents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().parse().unwrap())
        .collect();
    (event_id, part_ids)
}

async fn import(
    app: &axum::Router,
    session: &str,
    event_id: Uuid,
    body: Value,
) -> (StatusCode, Value) {
    let (status, report, _) = send(
        app,
        Method::POST,
        &format!("/api/events/{event_id}/guests/import"),
        Some(body),
        Some(session),
    )
    .await;
    (status, report)
}

async fn list(app: &axum::Router, session: &str, event_id: Uuid) -> Vec<Value> {
    let (status, body, _) = send(
        app,
        Method::GET,
        &format!("/api/events/{event_id}/guests"),
        None,
        Some(session),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body.as_array().unwrap().clone()
}

#[sqlx::test]
async fn guest_lifecycle_with_invites(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;
    let (event_id, parts) = seed_wedding(&app, &session).await;

    // Create, invited to the engagement and the reception but not the church.
    let (status, guest, _) = send(
        &app,
        Method::POST,
        &format!("/api/events/{event_id}/guests"),
        Some(json!({
            "name": "Adaeze Okafor",
            "phone": "0806 688 2563",
            "email": "ADA@Example.com",
            "plusOnes": 2,
            "dietary": "Vegetarian",
            "notes": "Bride's cousin",
            "subEventIds": [parts[0], parts[2]]
        })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{guest}");
    // Contact details are canonicalised on the way in, not as typed.
    assert_eq!(guest["phone"], "+2348066882563");
    assert_eq!(guest["email"], "ada@example.com");
    assert_eq!(guest["plusOnes"], 2);
    assert_eq!(guest["subEventIds"].as_array().unwrap().len(), 2);
    let guest_id = guest["id"].as_str().unwrap().to_string();

    // Update: swap the invitations wholesale, clear the email, keep the rest.
    let (status, updated, _) = send(
        &app,
        Method::PATCH,
        &format!("/api/guests/{guest_id}"),
        Some(json!({ "subEventIds": [parts[1]], "email": null, "plusOnes": 1 })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert!(updated["email"].is_null());
    assert_eq!(updated["phone"], "+2348066882563", "an untouched field survives");
    assert_eq!(updated["dietary"], "Vegetarian");
    assert_eq!(updated["plusOnes"], 1);
    assert_eq!(updated["subEventIds"], json!([parts[1]]));

    let (status, _, _) = send(
        &app,
        Method::DELETE,
        &format!("/api/guests/{guest_id}"),
        None,
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(list(&app, &session, event_id).await.is_empty());
}

#[sqlx::test]
async fn rejects_contact_details_it_could_never_reach(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;
    let (event_id, _) = seed_wedding(&app, &session).await;

    for bad in [
        json!({ "name": "A", "phone": "12345" }),
        json!({ "name": "A", "email": "not-an-email" }),
        json!({ "name": "", "phone": "08066882563" }),
        json!({ "name": "A", "plusOnes": 99 }),
        json!({ "name": "A", "plusOnes": -1 }),
    ] {
        let (status, body, _) = send(
            &app,
            Method::POST,
            &format!("/api/events/{event_id}/guests"),
            Some(bad.clone()),
            Some(&session),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad} was accepted: {body}");
    }
}

#[sqlx::test]
async fn a_phone_number_identifies_one_guest_per_event(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;
    let (event_id, _) = seed_wedding(&app, &session).await;

    let body = json!({ "name": "Adaeze", "phone": "08066882563" });
    let (status, _, _) = send(
        &app,
        Method::POST,
        &format!("/api/events/{event_id}/guests"),
        Some(body.clone()),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // The same human, typed differently, is still the same phone number.
    let (status, conflict, _) = send(
        &app,
        Method::POST,
        &format!("/api/events/{event_id}/guests"),
        Some(json!({ "name": "Ada O.", "phone": "+234 806 688 2563" })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");

    // But a different event is a different guest list.
    let (other_event, _) = seed_wedding(&app, &session).await;
    let (status, _, _) = send(
        &app,
        Method::POST,
        &format!("/api/events/{other_event}/guests"),
        Some(body),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[sqlx::test]
async fn import_creates_guests_and_reports_bad_rows(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;
    let (event_id, parts) = seed_wedding(&app, &session).await;

    let (status, report) = import(
        &app,
        &session,
        event_id,
        json!({
            "csv": "Name,Phone Number,Email,Plus Ones,Dietary,Table\n\
                   Adaeze Okafor,08066882563,ada@example.com,2,Vegetarian,4\n\
                   Tunde Bakare,+234 802 111 2222,,1,,7\n\
                   Broken Phone,12345,,0,,9\n\
                   ,08099999999,,0,,2\n",
            "subEventIds": [parts[2]],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["created"], 2);
    assert_eq!(report["updated"], 0);
    assert_eq!(report["errors"].as_array().unwrap().len(), 2);
    assert_eq!(report["errors"][0]["line"], 4, "points at the file line: {report}");
    assert_eq!(report["ignoredColumns"], json!(["Table"]), "{report}");

    let guests = list(&app, &session, event_id).await;
    assert_eq!(guests.len(), 2);
    assert_eq!(guests[0]["name"], "Adaeze Okafor");
    assert_eq!(guests[0]["phone"], "+2348066882563");
    assert_eq!(guests[0]["plusOnes"], 2);
    assert_eq!(guests[0]["subEventIds"], json!([parts[2]]), "invited to the chosen part");
}

#[sqlx::test]
async fn a_dry_run_writes_nothing(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;
    let (event_id, parts) = seed_wedding(&app, &session).await;

    let csv = "Name,Phone\nAdaeze,08066882563\nBroken,nonsense\n";
    let (status, report) = import(
        &app,
        &session,
        event_id,
        json!({ "csv": csv, "subEventIds": [parts[0]], "dryRun": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["dryRun"], true);
    assert_eq!(report["created"], 1, "reports what it would do");
    assert_eq!(report["errors"].as_array().unwrap().len(), 1);
    assert!(list(&app, &session, event_id).await.is_empty(), "but writes nothing");

    // Committing for real produces the same counts and actually lands.
    let (_, report) = import(
        &app,
        &session,
        event_id,
        json!({ "csv": csv, "subEventIds": [parts[0]] }),
    )
    .await;
    assert_eq!(report["created"], 1);
    assert_eq!(report["dryRun"], false);
    assert_eq!(list(&app, &session, event_id).await.len(), 1);
}

#[sqlx::test]
async fn reimporting_updates_guests_instead_of_duplicating_them(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;
    let (event_id, parts) = seed_wedding(&app, &session).await;

    let (_, report) = import(
        &app,
        &session,
        event_id,
        json!({
            "csv": "Name,Phone,Plus Ones\nAdaeze Okafor,08066882563,2\n",
            "subEventIds": [parts[0]],
        }),
    )
    .await;
    assert_eq!(report["created"], 1);

    // The organizer fixes a spelling and re-uploads. Same phone: same guest.
    let (_, report) = import(
        &app,
        &session,
        event_id,
        json!({
            "csv": "Name,Phone,Plus Ones\nAdaeze Okafor-Bello,+2348066882563,3\n",
            "subEventIds": [parts[0]],
        }),
    )
    .await;
    assert_eq!(report["created"], 0, "{report}");
    assert_eq!(report["updated"], 1);

    let guests = list(&app, &session, event_id).await;
    assert_eq!(guests.len(), 1, "not duplicated");
    assert_eq!(guests[0]["name"], "Adaeze Okafor-Bello");
    assert_eq!(guests[0]["plusOnes"], 3);
}

#[sqlx::test]
async fn a_file_without_a_column_does_not_erase_that_field(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;
    let (event_id, parts) = seed_wedding(&app, &session).await;

    import(
        &app,
        &session,
        event_id,
        json!({
            "csv": "Name,Phone,Plus Ones,Dietary,Notes\n\
                   Adaeze,08066882563,3,Vegetarian,Bride's cousin\n",
            "subEventIds": [parts[0]],
        }),
    )
    .await;

    // A later upload — a plain name-and-phone list — must not silently reset
    // the plus-ones, dietary needs and notes to nothing.
    let (_, report) = import(
        &app,
        &session,
        event_id,
        json!({ "csv": "Name,Phone\nAdaeze,08066882563\n", "subEventIds": [parts[0]] }),
    )
    .await;
    assert_eq!(report["updated"], 1, "{report}");

    let guests = list(&app, &session, event_id).await;
    assert_eq!(guests[0]["plusOnes"], 3, "plus-ones survived: {:?}", guests[0]);
    assert_eq!(guests[0]["dietary"], "Vegetarian");
    assert_eq!(guests[0]["notes"], "Bride's cousin");
}

#[sqlx::test]
async fn importing_one_list_per_part_accumulates_invitations(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;
    let (event_id, parts) = seed_wedding(&app, &session).await;

    // The reception list, then the engagement list: the same guest is on both.
    for part in [parts[2], parts[0]] {
        import(
            &app,
            &session,
            event_id,
            json!({ "csv": "Name,Phone\nAdaeze,08066882563\n", "subEventIds": [part] }),
        )
        .await;
    }

    let guests = list(&app, &session, event_id).await;
    assert_eq!(guests.len(), 1);
    let invited = guests[0]["subEventIds"].as_array().unwrap();
    assert_eq!(invited.len(), 2, "the second import did not undo the first: {invited:?}");
    assert!(invited.contains(&json!(parts[0])) && invited.contains(&json!(parts[2])));
}

#[sqlx::test]
async fn a_parts_column_overrides_the_chosen_parts(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;
    let (event_id, parts) = seed_wedding(&app, &session).await;

    let (status, report) = import(
        &app,
        &session,
        event_id,
        json!({
            // Named however the organizer likes: the resolution is by slug.
            "csv": "Name,Phone,Attending\n\
                   Adaeze,08066882563,\"Church Ceremony, Reception\"\n\
                   Tunde,08022221111,CHURCH-CEREMONY\n\
                   Ngozi,08033334444,\n\
                   Bola,08044445555,Boat Cruise\n",
            "subEventIds": [parts[0]],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["created"], 3, "the unknown part fails only its own row: {report}");
    assert_eq!(report["unknownParts"], json!(["Boat Cruise"]), "{report}");

    let guests = list(&app, &session, event_id).await;
    let by_name = |name: &str| -> Value {
        guests.iter().find(|g| g["name"] == name).unwrap()["subEventIds"].clone()
    };
    assert_eq!(by_name("Adaeze").as_array().unwrap().len(), 2);
    assert_eq!(by_name("Tunde"), json!([parts[1]]));
    assert_eq!(by_name("Ngozi"), json!([parts[0]]), "no parts cell falls back to the chosen part");
    assert!(guests.iter().all(|g| g["name"] != "Bola"));
}

#[sqlx::test]
async fn import_rejects_a_file_it_cannot_read(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;
    let (event_id, _) = seed_wedding(&app, &session).await;

    for csv in ["", "Table,Seat\n1,2\n"] {
        let (status, body) = import(&app, &session, event_id, json!({ "csv": csv })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{csv:?} was accepted: {body}");
    }
}

#[sqlx::test]
async fn guests_cannot_be_invited_across_events(pool: PgPool) {
    let app = app(pool.clone());
    let (_, session) = seed_user(&pool, "org@example.com").await;
    let (event_id, _) = seed_wedding(&app, &session).await;
    let (_, other_parts) = seed_wedding(&app, &session).await;

    // Both events belong to the same organizer, so ownership alone wouldn't
    // catch this: the invitation itself has to be impossible.
    let (status, body, _) = send(
        &app,
        Method::POST,
        &format!("/api/events/{event_id}/guests"),
        Some(json!({ "name": "Adaeze", "subEventIds": [other_parts[0]] })),
        Some(&session),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(list(&app, &session, event_id).await.is_empty(), "and no half-made guest is left");
}

#[sqlx::test]
async fn guest_lists_are_shared_but_require_a_session(pool: PgPool) {
    let app = app(pool.clone());
    // Both are staff of the one agency; a guest list is shared between them.
    let (_, founder) = seed_user(&pool, "founder@example.com").await;
    let (_, coordinator) = seed_user(&pool, "coordinator@example.com").await;
    let (event_id, parts) = seed_wedding(&app, &founder).await;

    let (_, guest, _) = send(
        &app,
        Method::POST,
        &format!("/api/events/{event_id}/guests"),
        Some(json!({ "name": "Adaeze", "phone": "08066882563", "subEventIds": [parts[0]] })),
        Some(&founder),
    )
    .await;
    let guest_id = guest["id"].as_str().unwrap();

    // A colleague works the same list: they can read it, add to it, and edit it.
    let guests = list(&app, &coordinator, event_id).await;
    assert_eq!(guests.len(), 1);
    assert_eq!(guests[0]["name"], "Adaeze");

    let (status, _, _) = send(
        &app,
        Method::PATCH,
        &format!("/api/guests/{guest_id}"),
        Some(json!({ "notes": "confirmed by phone" })),
        Some(&coordinator),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // But every one of these still needs a valid session.
    for (method, uri, body) in [
        (Method::GET, format!("/api/events/{event_id}/guests"), None),
        (
            Method::POST,
            format!("/api/events/{event_id}/guests"),
            Some(json!({ "name": "Gatecrasher" })),
        ),
        (Method::PATCH, format!("/api/guests/{guest_id}"), Some(json!({ "name": "Hijacked" }))),
        (Method::DELETE, format!("/api/guests/{guest_id}"), None),
    ] {
        let (status, _, _) = send(&app, method, &uri, body, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
}
