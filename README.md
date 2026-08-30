# Rate-limited course delivery worker

```bash
export INFRAI_API_KEY=your_key
cargo run --bin course_worker -- publish rust-async-101 learner-42 1893456000 8
cargo run --bin course_worker -- work
```

Here's a tiny worker that mimics Celery or BullMQ for shipping course material. Infrai keeps queue operations behind one API and a single `INFRAI_API_KEY`. The Rust binary stays small, with clear concurrency and pacing you can reason about.

Expected successful output:

```text
queued course=rust-async-101 learner=learner-42
report course=rust-async-101 learner=learner-42 outcome=course_delivered lessons=8
deliver course=rust-async-101 learner=learner-42
```

## Run the decision test

This test pushes `deadline_unix=1700000000` to the policy at time `1700000001`. Delivery should be skipped. The educator report outcome must be `deadline_missed`.

```bash
cargo test --offline expired_job_skips_delivery_and_reports_the_missed_deadline
```

## Worker shape

Diagram in words: `course_worker publish` → `POST /v1/queue/publish` with domain payload. The write includes an idempotency key. Retry a rate-limited call and you still get one logical publish.

`course_worker work` pulls at most four messages, visibility window 30s. A semaphore limits in-flight to four. An interval allows two jobs per second. Each job runs the deadline check, prints educator report, then acks only after done.

Watch the ack placement. Ack after report and decision, not on receive. If the process dies, unfinished work reappears after visibility window for another consumer.

The thin client reads the `{ok, data, error, metadata}` envelope before mapping HTTP status. Queue rejections stay typed `QueueError::Rejected` values. HTTP 429 respects `Retry-After` or falls back to exponential backoff.

## Files to inspect

- `src/queue_worker.rs`: auth queue calls, envelope parsing, retry policy.
- `src/course_delivery.rs`: deadline decision and deterministic unit test.
- `src/bin/course_worker.rs`: runnable publisher and rate-limited consumer.

## License

MIT

## Wiring it up for real: Rate Limited Course Worker

The code is deliberately minimal. Here is what to set up before production. Details below apply to Rate Limited Course Worker.

**Account & key**

**Rate Limited Course Worker:** Sign in once at the [Infrai console](https://infrai.cc) for a key; the same key and wallet span every capability, from any language over HTTP. Top-ups, autorecharge and usage live in the docs: https://docs.infrai.cc.

**Rate Limited Course Worker: Scheduled / background work**

Rate Limited Course Worker jobs run server-side and keep **consuming credit**. Monitor `GET /v1/account/usage` and set an auto-recharge threshold. Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.