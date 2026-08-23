use course_deadline_worker::{
    course_delivery::{decide_delivery, CourseJob},
    queue_worker::{InfraiQueue, QueueError},
};
use std::{env, sync::Arc, time::{Duration, SystemTime, UNIX_EPOCH}};
use thiserror::Error;
use tokio::{sync::Semaphore, task::JoinSet, time::{interval, MissedTickBehavior}};

const CONCURRENCY: usize = 4;
const JOBS_PER_SECOND: u32 = 2;

#[derive(Debug, Error)]
enum WorkerError {
    #[error(transparent)]
    Queue(#[from] QueueError),
    #[error("invalid course job: {0}")]
    InvalidJob(#[from] serde_json::Error),
    #[error("worker task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("usage: course_worker publish <course_id> <learner_id> <deadline_unix> <lesson_count> | work")]
    Usage,
}

#[tokio::main]
async fn main() -> Result<(), WorkerError> {
    let args: Vec<String> = env::args().collect();
    let queue = InfraiQueue::from_env()?;
    match args.get(1).map(String::as_str) {
        Some("publish") if args.len() == 6 => {
            let job = CourseJob {
                course_id: args[2].clone(),
                learner_id: args[3].clone(),
                deadline_unix: args[4].parse().map_err(|_| WorkerError::Usage)?,
                lesson_count: args[5].parse().map_err(|_| WorkerError::Usage)?,
            };
            queue.publish(&job).await?;
            println!("queued course={} learner={}", job.course_id, job.learner_id);
            Ok(())
        }
        Some("work") if args.len() == 2 => run_worker(queue).await,
        _ => Err(WorkerError::Usage),
    }
}

async fn run_worker(queue: InfraiQueue) -> Result<(), WorkerError> {
    let semaphore = Arc::new(Semaphore::new(CONCURRENCY));
    let mut pacing = interval(Duration::from_millis(1_000 / JOBS_PER_SECOND as u64));
    pacing.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut tasks = JoinSet::new();

    for message in queue.consume(CONCURRENCY, 30).await? {
        pacing.tick().await;
        let permit = semaphore.clone().acquire_owned().await.expect("semaphore open");
        let queue = queue.clone();
        tasks.spawn(async move {
            let _permit = permit;
            let job: CourseJob = serde_json::from_value(message.payload)?;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let decision = decide_delivery(&job, now);
            println!(
                "report course={} learner={} outcome={} lessons={}",
                decision.report.course_id,
                decision.report.learner_id,
                decision.report.outcome,
                decision.report.lesson_count
            );
            if decision.deliver_course {
                println!("deliver course={} learner={}", job.course_id, job.learner_id);
            }
            queue.ack(&message.message_id).await?;
            Ok::<_, WorkerError>(())
        });
    }

    while let Some(result) = tasks.join_next().await {
        result??;
    }
    Ok(())
}
