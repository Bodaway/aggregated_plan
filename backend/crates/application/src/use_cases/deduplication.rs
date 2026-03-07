use std::collections::HashSet;

use chrono::Utc;
use domain::rules::dedup::{calculate_similarity, find_jira_key_in_text, DEDUP_CONFIDENCE_THRESHOLD};
use domain::types::*;
use uuid::Uuid;

use crate::errors::AppError;
use crate::repositories::*;

/// A suggested duplicate pair with its similarity details.
pub struct DeduplicationSuggestion {
    pub id: Uuid,
    pub task_a: Task,
    pub task_b: Task,
    pub confidence_score: f64,
    pub title_similarity: f64,
    pub assignee_match: bool,
    pub project_match: bool,
}

/// Detect potential duplicate tasks for a user.
pub async fn find_suggestions(
    task_repo: &dyn TaskRepository,
    task_link_repo: &dyn TaskLinkRepository,
    user_id: UserId,
) -> Result<Vec<DeduplicationSuggestion>, AppError> {
    // 1. Get all active tasks for the user
    let filter = TaskFilter {
        status: Some(vec![TaskStatus::Todo, TaskStatus::InProgress]),
        ..TaskFilter::empty()
    };
    let tasks = task_repo.find_by_user(user_id, &filter).await?;

    // 2. Get rejected pairs to exclude
    let rejected_pairs = task_link_repo.find_rejected_pairs(user_id).await?;
    let rejected_set: HashSet<(TaskId, TaskId)> = rejected_pairs
        .iter()
        .flat_map(|(a, b)| vec![(*a, *b), (*b, *a)])
        .collect();

    // 3. Get existing links to exclude
    let existing_links = task_link_repo.find_by_user(user_id).await?;
    let linked_set: HashSet<(TaskId, TaskId)> = existing_links
        .iter()
        .filter(|l| l.link_type != TaskLinkType::Rejected)
        .flat_map(|l| {
            vec![
                (l.task_id_primary, l.task_id_secondary),
                (l.task_id_secondary, l.task_id_primary),
            ]
        })
        .collect();

    let mut suggestions = Vec::new();

    // 4. For each pair (i, j) where i < j
    for i in 0..tasks.len() {
        for j in (i + 1)..tasks.len() {
            let task_a = &tasks[i];
            let task_b = &tasks[j];

            // Check if already linked or rejected
            if linked_set.contains(&(task_a.id, task_b.id)) {
                continue;
            }
            if rejected_set.contains(&(task_a.id, task_b.id)) {
                continue;
            }

            // R08: Jira key match
            let mut jira_match = false;
            if let Some(ref source_id) = task_a.source_id {
                if task_a.source == Source::Jira {
                    if find_jira_key_in_text(source_id, &task_b.title)
                        || task_b
                            .description
                            .as_deref()
                            .map(|d| find_jira_key_in_text(source_id, d))
                            .unwrap_or(false)
                    {
                        jira_match = true;
                    }
                }
            }
            if !jira_match {
                if let Some(ref source_id) = task_b.source_id {
                    if task_b.source == Source::Jira {
                        if find_jira_key_in_text(source_id, &task_a.title)
                            || task_a
                                .description
                                .as_deref()
                                .map(|d| find_jira_key_in_text(source_id, d))
                                .unwrap_or(false)
                        {
                            jira_match = true;
                        }
                    }
                }
            }

            if jira_match {
                suggestions.push(DeduplicationSuggestion {
                    id: Uuid::new_v4(),
                    task_a: task_a.clone(),
                    task_b: task_b.clone(),
                    confidence_score: 1.0,
                    title_similarity: 1.0,
                    assignee_match: true,
                    project_match: true,
                });
                continue;
            }

            // R09: Similarity scoring
            let project_a = task_a.project_id.map(|p| p.to_string());
            let project_b = task_b.project_id.map(|p| p.to_string());

            let score = calculate_similarity(
                &task_a.title,
                &task_b.title,
                task_a.assignee.as_deref(),
                task_b.assignee.as_deref(),
                project_a.as_deref(),
                project_b.as_deref(),
            );

            if score.overall >= DEDUP_CONFIDENCE_THRESHOLD {
                suggestions.push(DeduplicationSuggestion {
                    id: Uuid::new_v4(),
                    task_a: task_a.clone(),
                    task_b: task_b.clone(),
                    confidence_score: score.overall,
                    title_similarity: score.title_score,
                    assignee_match: score.assignee_match,
                    project_match: score.project_match,
                });
            }
        }
    }

    // 5. Sort by confidence descending
    suggestions.sort_by(|a, b| {
        b.confidence_score
            .partial_cmp(&a.confidence_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(suggestions)
}

/// Confirm or reject a deduplication suggestion.
pub async fn confirm_suggestion(
    task_link_repo: &dyn TaskLinkRepository,
    task_id_primary: TaskId,
    task_id_secondary: TaskId,
    accept: bool,
) -> Result<(), AppError> {
    let link_type = if accept {
        TaskLinkType::AutoMerged
    } else {
        TaskLinkType::Rejected
    };

    let link = TaskLink {
        id: Uuid::new_v4(),
        task_id_primary,
        task_id_secondary,
        link_type,
        confidence_score: None,
        created_at: Utc::now(),
    };

    task_link_repo.save(&link).await?;
    Ok(())
}

/// Manually link two tasks.
pub async fn link_tasks(
    task_link_repo: &dyn TaskLinkRepository,
    task_id_primary: TaskId,
    task_id_secondary: TaskId,
) -> Result<(), AppError> {
    let link = TaskLink {
        id: Uuid::new_v4(),
        task_id_primary,
        task_id_secondary,
        link_type: TaskLinkType::ManualMerged,
        confidence_score: None,
        created_at: Utc::now(),
    };

    task_link_repo.save(&link).await?;
    Ok(())
}

/// Unlink two tasks by deleting their link.
pub async fn unlink_tasks(
    task_link_repo: &dyn TaskLinkRepository,
    link_id: TaskLinkId,
) -> Result<(), AppError> {
    task_link_repo.delete(link_id).await?;
    Ok(())
}
