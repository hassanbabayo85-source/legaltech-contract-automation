# Demo workflow

A deterministic, self-contained demo of the full LexHack pipeline.
Everything runs locally; no external services are required.

## Prerequisites

* PostgreSQL running locally (or use `docker compose up postgres`).
* Backend built: `cargo build`.
* Frontend built: `cd frontend && trunk build`.
* `curl` and `jq` for the seed script.

## One-time setup

    # 1. Create the database and role.
    sudo -u postgres psql <<'SQL'
    DROP DATABASE IF EXISTS lexhack_demo;
    DROP USER IF EXISTS lexhack_demo;
    CREATE USER lexhack_demo WITH PASSWORD 'lexhack_demo_pw' CREATEDB;
    CREATE DATABASE lexhack_demo OWNER lexhack_demo;
    \c lexhack_demo
    GRANT ALL ON SCHEMA public TO lexhack_demo;
    SQL

    # 2. Point the backend at it.
    export DATABASE_URL="postgres://lexhack_demo:lexhack_demo_pw@127.0.0.1:5432/lexhack_demo"
    export NOTIFICATION_SECRET_KEY="$(openssl rand -base64 32)"
    export CORS_ALLOWED_ORIGINS="http://localhost:8080"

    # 3. Apply migrations.
    cargo run -- --migrate

## Running the demo

### Terminal 1 — backend + worker

    cd ~/lexhack-backend
    export DATABASE_URL="postgres://lexhack_demo:lexhack_demo_pw@127.0.0.1:5432/lexhack_demo"
    export NOTIFICATION_SECRET_KEY="$(openssl rand -base64 32)"
    export CORS_ALLOWED_ORIGINS="http://localhost:8080"
    cargo run

The backend listens on `http://localhost:3000` and the background
worker starts automatically.

### Terminal 2 — frontend

    cd ~/lexhack-backend/frontend
    trunk serve

The frontend listens on `http://localhost:8080`.

### Terminal 3 — seed demo data

    cd ~/lexhack-backend
    ./scripts/seed_demo.sh

The script prints the demo account credentials and a contract ID.
It is safe to re-run.

## Demo script

Follow this flow; each step is fast and predictable.

### 1. Sign in

Open `http://localhost:8080/login`, sign in with:

* Email: `demo@lexhack.local`
* Password: `correct horse battery staple`

You land on the dashboard.

### 2. Dashboard

The dashboard shows aggregate counts (contracts, reminders,
channels). The demo contract should appear in the list after the
next step.

### 3. Open the demo contract

* Click **Contracts** in the sidebar.
* Click the demo contract.
* The header shows status `Not analyzed`, no risk level yet, and the
  start/end dates (from the fictional text — actually still null
  because analysis hasn't run).

### 4. Run AI analysis

* Click **Analyze contract**.
* If `AI_API_KEY` is unset, the request returns **502**. Set it and
  retry — the analysis is idempotent and safe to re-run.
* When configured, the button shows **Analyzing… this may take a
  moment** and the response arrives in a few seconds.

After success:

* Status becomes **Completed**.
* Risk badge shows **High 75/100**.
* One risk appears: *"Automatic renewal"*, with an **Evidence**
  excerpt from Clause 4.2.
* One obligation appears: *"Payment due"* with a due date 30 days
  in the future.
* Contract start/end dates are now filled (`2026-01-01` /
  `2026-12-31`).

### 5. Generate reminders

* Scroll to the **Reminders** section.
* Click **Generate reminders**.
* The response message shows `4 created, 0 preserved`.
* Four reminders appear: `7 days before`, `3 days before`,
  `1 day before`, `On deadline`.

**Idempotency check:** click **Generate reminders** again. The
message now shows `0 created, 4 preserved`. The table has not grown.

### 6. Add a custom reminder (optional)

* Click **Add custom**.
* Pick the payment obligation.
* Pick a date a few days from now.
* Pick the demo webhook channel.
* Click **Schedule reminder**.

The new reminder appears in the list with source `custom`.

### 7. Notification channel

* Click **Channels** in the sidebar.
* The demo webhook channel is listed with status **Completed**
  (enabled) and **Configured (encrypted)**.
* Click **Send test** to exercise the SSRF gate. The demo URL
  (`https://example.com/lexhack/demo-hook`) resolves to a public IP,
  so the request is *sent*, but example.com will return 404. The
  toast says **Test failed: permanent_http_error** — that is
  expected and shows the error path works.

### 8. Worker delivery (background)

The worker polls every 10 seconds by default. To force a delivery
attempt during the demo:

    # In a psql shell on lexhack_demo:
    UPDATE reminders
       SET reminder_date = NOW() - INTERVAL '1 minute'
     WHERE id = (
         SELECT id
           FROM reminders
          WHERE status = 'pending'
          ORDER BY reminder_date
          LIMIT 1
     );

Wait 10–15 seconds. In the backend's stdout you will see structured
logs:

    WARN lexhack_backend::worker: notification_failed reminder_id=…
      channel="webhook" category="permanent_http_error" retryable=false

The reminder row's `attempts` is now `1`. Because the failure is
permanent, the status is `failed`.

For a retryable failure, point the channel at an address that is
reachable but slow, or temporarily stop the network; the worker
will schedule a retry with exponential backoff.

### 9. Audit trail

In psql:

    SELECT action, entity_id, created_at
      FROM audit_logs
     ORDER BY created_at;

You should see (at minimum):

* `contract_created`
* `contract_analyzed`
* `contract_reminders_regenerated`
* `reminder_created` (if you added a custom one)

Each row's `metadata` is safe — no contract text, no secrets.

## Resetting the demo

    dropdb lexhack_demo
    createdb -O lexhack_demo lexhack_demo
    cargo run -- --migrate
    ./scripts/seed_demo.sh

## Fictional data only

The demo contract text is clearly fictional. Do **not** run the seed
script against any database that contains real user data — it uses a
fixed demo email address and would conflict with a real account.
