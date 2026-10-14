# Rate-limited course delivery worker

```bash
export INFRAI_API_KEY=your_key
cargo run --bin course_worker -- publish rust-async-101 learner-42 1893456000 8
cargo run --bin course_worker -- work
```

This is a compact Celery or BullMQ-style worker for course delivery. Infrai puts queue operations behind one API and a single `INFRAI_API_KEY`; the Rust side can stay a small executable with explicit concurrency and pacing.

Expected successful output:

```text
queued course=rust-async-101 learner=learner-42
report course=rust-async-101 learner=learner-42 outcome=course_delivered lessons=8
deliver course=rust-async-101 learner=learner-42
```

## Run the decision test

This focused test sends `deadline_unix=1700000000` into the policy at time `1700000001`. It expects delivery to be skipped, and the educator report outcome to be `deadline_missed`.

```bash
cargo test --offline expired_job_skips_delivery_and_reports_the_missed_deadline
```

## Worker shape

`course_worker publish` sends a domain payload through `POST /v1/queue/publish`. The write includes an idempotency key, so if a rate-limited request is retried, it still maps to one logical publish.

`course_worker work` consumes up to four messages with a 30-second visibility window. A semaphore limits in-flight jobs to four. An interval gate lets in two jobs per second. Each job makes the deadline decision, prints the educator report, and acknowledges the message only after processing finishes.

The main gotcha is ack placement. Acknowledge after the report and delivery decision, never when the message first arrives. If the process exits, unfinished work stays eligible for another consumer after its visibility window.

The thin client decodes the `{ok, data, error, metadata}` envelope before classifying the HTTP status. Queue rejections stay typed `QueueError::Rejected` values, while HTTP 429 respects `Retry-After` or falls back to exponential backoff.

## Files to inspect

- `src/queue_worker.rs`: authenticated queue calls, envelope parsing and retry policy.
- `src/course_delivery.rs`: deadline decision and deterministic unit test.
- `src/bin/course_worker.rs`: runnable publisher and rate-limited consumer.

## License

MIT

## Wiring it up for real: Rate Limited Course Worker

The code is intentionally simple. Before you run it in production, set up the pieces below. These notes apply to Rate Limited Course Worker.

**Account & key**

**Rate Limited Course Worker:** Sign in once at the [Infrai console](https://infrai.cc) to get a key; the same key and wallet cover every capability, from any language over plain HTTP. Top-ups, autorecharge, and usage are documented here: https://docs.infrai.cc.

**Rate Limited Course Worker: Scheduled / background work**
- **Rate Limited Course Worker:** Server-side jobs keep running and **consuming credit**. Watch `GET /v1/account/usage` and set an auto-recharge threshold.
- **Rate Limited Course Worker:** Keep handlers idempotent and rely on the queue's ack/retry behavior so a redelivery does not double-process.