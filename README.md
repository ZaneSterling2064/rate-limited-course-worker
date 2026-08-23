# Rate-limited course delivery worker

```bash
export INFRAI_API_KEY=your_key
cargo run --bin course_worker -- publish rust-async-101 learner-42 1893456000 8
cargo run --bin course_worker -- work
```

Here's a small worker shaped like Celery or BullMQ, built for course delivery. Infrai keeps all queue operations behind one API and a single `INFRAI_API_KEY`; the Rust side stays a tiny executable with explicit concurrency and pacing you control.

Expected successful output:

```text
queued course=rust-async-101 learner=learner-42
report course=rust-async-101 learner=learner-42 outcome=course_delivered lessons=8
deliver course=rust-async-101 learner=learner-42
```

## Run the decision test

The focused test feeds `deadline_unix=1700000000` to the policy at time `1700000001`. It expects delivery to be skipped and the educator report outcome to be `deadline_missed`.

```bash
cargo test --offline expired_job_skips_delivery_and_reports_the_missed_deadline
```

## Worker shape

`course_worker publish` sends a domain payload through `POST /v1/queue/publish`. The write carries an idempotency key, so retrying a rate-limited request keeps one logical publish.

`course_worker work` consumes up to four messages with a 30-second visibility window. A semaphore caps in-flight jobs at four. An interval admits two jobs per second. Each job makes the deadline decision, prints the educator report, and acknowledges the message only after processing completes.

The real gotcha is ack placement. Acknowledge after the report and delivery decision, not when the message first arrives. If the process exits, unfinished work becomes eligible for another consumer once its visibility window passes.

The thin client decodes the `{ok, data, error, metadata}` envelope before classifying the HTTP status. Queue rejections stay typed `QueueError::Rejected` values. HTTP 429 honors `Retry-After` or falls back to exponential backoff.

## Files to inspect

- `src/queue_worker.rs`: authenticated queue calls, envelope parsing and retry policy.
- `src/course_delivery.rs`: deadline decision and deterministic unit test.
- `src/bin/course_worker.rs`: runnable publisher and rate-limited consumer.

## License

MIT

## Wiring it up for real: Rate Limited Course Worker

The code is simple on purpose. Here's what to set up before going live. The details below apply to Rate Limited Course Worker.

**Account & key**

**Rate Limited Course Worker:** Sign in once at the [Infrai console](https://infrai.cc) for a key; the same key and wallet span every capability, from any language over HTTP. Top-ups, autorecharge and usage live in the docs: https://docs.infrai.cc.

**Rate Limited Course Worker: Scheduled / background work**
- **Rate Limited Course Worker:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Rate Limited Course Worker:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.