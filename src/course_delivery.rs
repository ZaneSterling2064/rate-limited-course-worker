use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CourseJob {
    pub course_id: String,
    pub learner_id: String,
    pub deadline_unix: u64,
    pub lesson_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeliveryDecision {
    pub deliver_course: bool,
    pub report: EducatorReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EducatorReport {
    pub course_id: String,
    pub learner_id: String,
    pub outcome: &'static str,
    pub lesson_count: u32,
}

pub fn decide_delivery(job: &CourseJob, now_unix: u64) -> DeliveryDecision {
    let before_deadline = now_unix <= job.deadline_unix;
    DeliveryDecision {
        deliver_course: before_deadline,
        report: EducatorReport {
            course_id: job.course_id.clone(),
            learner_id: job.learner_id.clone(),
            outcome: if before_deadline {
                "course_delivered"
            } else {
                "deadline_missed"
            },
            lesson_count: job.lesson_count,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_job_skips_delivery_and_reports_the_missed_deadline() {
        let job = CourseJob {
            course_id: "rust-async-101".into(),
            learner_id: "learner-42".into(),
            deadline_unix: 1_700_000_000,
            lesson_count: 8,
        };

        let decision = decide_delivery(&job, 1_700_000_001);

        assert!(!decision.deliver_course);
        assert_eq!(decision.report.outcome, "deadline_missed");
        assert_eq!(decision.report.lesson_count, 8);
    }
}

