//! Per-profile to-dos with an optional due time (fork).
//!
//! Kept in one JSON file of the app's data directory, on this machine only:
//! tasks are never synced and never leave the app. The file is replaced
//! atomically, and every command reads it fresh under one lock, so two
//! commands never overwrite each other's change.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

const FILE_NAME: &str = "profile_tasks.json";
const MAX_TITLE_CHARS: usize = 200;
const MAX_TASKS_PER_PROFILE: usize = 200;
const MAX_TASKS_TOTAL: usize = 5000;

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileTask {
  pub id: String,
  pub profile_id: String,
  pub title: String,
  /// Unix seconds. `None` means the task has no due time.
  #[serde(default)]
  pub due_at: Option<u64>,
  #[serde(default)]
  pub done: bool,
  /// Set once the reminder for the current due time was shown, so a restart
  /// does not repeat it. Changing the due time clears it.
  #[serde(default)]
  pub reminded: bool,
  pub created_at: u64,
  #[serde(default)]
  pub updated_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TaskFile {
  #[serde(default)]
  tasks: Vec<ProfileTask>,
}

/// What an edit changes. Absent fields stay as they are.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPatch {
  pub title: Option<String>,
  pub done: Option<bool>,
  pub due_at: Option<u64>,
  #[serde(default)]
  pub clear_due: bool,
}

fn code(code: &str) -> String {
  crate::backend_error(code)
}

fn file_path() -> PathBuf {
  crate::app_dirs::data_subdir().join(FILE_NAME)
}

fn read_tasks() -> Result<Vec<ProfileTask>, String> {
  let path = file_path();
  let text = match std::fs::read_to_string(&path) {
    Ok(text) => text,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
    Err(e) => {
      return Err(crate::backend_error_with_detail(
        "PROFILE_TASKS_LOAD_FAILED",
        e,
      ))
    }
  };
  serde_json::from_str::<TaskFile>(&text)
    .map(|file| file.tasks)
    .map_err(|e| crate::backend_error_with_detail("PROFILE_TASKS_LOAD_FAILED", e))
}

fn write_tasks(tasks: &[ProfileTask]) -> Result<(), String> {
  let text = serde_json::to_string_pretty(&TaskFile {
    tasks: tasks.to_vec(),
  })
  .map_err(|e| crate::backend_error_with_detail("PROFILE_TASKS_SAVE_FAILED", e))?;
  crate::camoufox::write_atomically(&file_path(), &text)
    .map_err(|e| crate::backend_error_with_detail("PROFILE_TASKS_SAVE_FAILED", e))
}

/// Reads the file, applies `change` and writes the result back when it
/// returned `Ok`, all under the lock.
fn mutate<T>(
  change: impl FnOnce(Vec<ProfileTask>) -> Result<(Vec<ProfileTask>, T), String>,
) -> Result<T, String> {
  let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
  let (tasks, value) = change(read_tasks()?)?;
  write_tasks(&tasks)?;
  Ok(value)
}

fn checked_title(title: &str) -> Result<String, String> {
  let title = title.trim();
  if title.is_empty() {
    return Err(code("PROFILE_TASK_TITLE_EMPTY"));
  }
  if title.chars().count() > MAX_TITLE_CHARS {
    return Err(code("PROFILE_TASK_TITLE_TOO_LONG"));
  }
  Ok(title.to_string())
}

/// Open tasks first (soonest due first, undated last), finished ones after,
/// newest first.
fn sort_for_display(tasks: &mut [ProfileTask]) {
  tasks.sort_by(|a, b| {
    a.done
      .cmp(&b.done)
      .then_with(|| match (a.done, a.due_at, b.due_at) {
        (false, Some(x), Some(y)) => x.cmp(&y),
        (false, Some(_), None) => std::cmp::Ordering::Less,
        (false, None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
      })
      .then_with(|| b.created_at.cmp(&a.created_at))
  });
}

fn with_added(
  tasks: &[ProfileTask],
  profile_id: &str,
  title: &str,
  due_at: Option<u64>,
  now: u64,
) -> Result<Vec<ProfileTask>, String> {
  let title = checked_title(title)?;
  let own = tasks.iter().filter(|t| t.profile_id == profile_id).count();
  if own >= MAX_TASKS_PER_PROFILE || tasks.len() >= MAX_TASKS_TOTAL {
    return Err(code("PROFILE_TASK_LIMIT_REACHED"));
  }
  let mut next = tasks.to_vec();
  next.push(ProfileTask {
    id: uuid::Uuid::new_v4().to_string(),
    profile_id: profile_id.to_string(),
    title,
    due_at,
    done: false,
    reminded: false,
    created_at: now,
    updated_at: now,
  });
  Ok(next)
}

fn with_patched(
  tasks: &[ProfileTask],
  task_id: &str,
  patch: &TaskPatch,
  now: u64,
) -> Result<Vec<ProfileTask>, String> {
  let title = patch.title.as_deref().map(checked_title).transpose()?;
  let mut found = false;
  let next = tasks
    .iter()
    .map(|task| {
      if task.id != task_id {
        return task.clone();
      }
      found = true;
      let mut task = task.clone();
      if let Some(title) = &title {
        task.title = title.clone();
      }
      if let Some(done) = patch.done {
        task.done = done;
      }
      if patch.clear_due {
        task.due_at = None;
        task.reminded = false;
      } else if let Some(due) = patch.due_at.filter(|due| Some(*due) != task.due_at) {
        task.due_at = Some(due);
        task.reminded = false;
      }
      task.updated_at = now;
      task
    })
    .collect();
  if found {
    Ok(next)
  } else {
    Err(code("PROFILE_TASK_NOT_FOUND"))
  }
}

fn with_reminded(tasks: &[ProfileTask], ids: &[String]) -> Vec<ProfileTask> {
  tasks
    .iter()
    .map(|task| {
      let mut task = task.clone();
      if ids.contains(&task.id) {
        task.reminded = true;
      }
      task
    })
    .collect()
}

/// Every task, or the tasks of one profile, in display order.
#[tauri::command]
pub fn list_profile_tasks(profile_id: Option<String>) -> Result<Vec<ProfileTask>, String> {
  let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
  let mut tasks: Vec<ProfileTask> = read_tasks()?
    .into_iter()
    .filter(|task| profile_id.as_deref().is_none_or(|id| task.profile_id == id))
    .collect();
  sort_for_display(&mut tasks);
  Ok(tasks)
}

#[tauri::command]
pub fn add_profile_task(
  profile_id: String,
  title: String,
  due_at: Option<u64>,
) -> Result<ProfileTask, String> {
  let now = crate::proxy_manager::now_secs();
  mutate(|tasks| {
    let next = with_added(&tasks, &profile_id, &title, due_at, now)?;
    let added = next
      .last()
      .cloned()
      .ok_or_else(|| code("PROFILE_TASK_NOT_FOUND"))?;
    Ok((next, added))
  })
}

#[tauri::command]
pub fn update_profile_task(task_id: String, patch: TaskPatch) -> Result<ProfileTask, String> {
  let now = crate::proxy_manager::now_secs();
  mutate(|tasks| {
    let next = with_patched(&tasks, &task_id, &patch, now)?;
    let updated = next
      .iter()
      .find(|t| t.id == task_id)
      .cloned()
      .ok_or_else(|| code("PROFILE_TASK_NOT_FOUND"))?;
    Ok((next, updated))
  })
}

#[tauri::command]
pub fn delete_profile_task(task_id: String) -> Result<(), String> {
  mutate(|tasks| {
    let before = tasks.len();
    let next: Vec<ProfileTask> = tasks.into_iter().filter(|t| t.id != task_id).collect();
    if next.len() == before {
      return Err(code("PROFILE_TASK_NOT_FOUND"));
    }
    Ok((next, ()))
  })
}

/// Drops every task of a profile, for when the profile itself is deleted.
pub fn delete_profile_tasks(profile_id: String) -> Result<u64, String> {
  mutate(|tasks| {
    let before = tasks.len();
    let next: Vec<ProfileTask> = tasks
      .into_iter()
      .filter(|t| t.profile_id != profile_id)
      .collect();
    let removed = (before - next.len()) as u64;
    Ok((next, removed))
  })
}

/// Marks reminders as shown so they are not shown again.
#[tauri::command]
pub fn mark_profile_tasks_reminded(task_ids: Vec<String>) -> Result<(), String> {
  if task_ids.is_empty() {
    return Ok(());
  }
  mutate(|tasks| {
    let next = with_reminded(&tasks, &task_ids);
    Ok((next, ()))
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn added(
    tasks: &[ProfileTask],
    profile: &str,
    title: &str,
    due: Option<u64>,
    now: u64,
  ) -> Vec<ProfileTask> {
    with_added(tasks, profile, title, due, now).expect("task added")
  }

  #[test]
  fn adding_trims_the_title_and_refuses_empty_or_long_ones() {
    let tasks = added(&[], "p1", "  call the host  ", Some(10), 1);
    assert_eq!(tasks[0].title, "call the host");
    assert_eq!(tasks[0].due_at, Some(10));
    assert!(!tasks[0].reminded);

    let empty = with_added(&[], "p1", "   ", None, 1).unwrap_err();
    assert!(empty.contains("PROFILE_TASK_TITLE_EMPTY"));
    let long = with_added(&[], "p1", &"x".repeat(MAX_TITLE_CHARS + 1), None, 1).unwrap_err();
    assert!(long.contains("PROFILE_TASK_TITLE_TOO_LONG"));
  }

  #[test]
  fn a_profile_holds_a_bounded_number_of_tasks() {
    let mut tasks = Vec::new();
    for n in 0..MAX_TASKS_PER_PROFILE {
      tasks = added(&tasks, "p1", &format!("task {n}"), None, 1);
    }
    let over = with_added(&tasks, "p1", "one more", None, 1).unwrap_err();
    assert!(over.contains("PROFILE_TASK_LIMIT_REACHED"));
    assert!(with_added(&tasks, "p2", "another profile", None, 1).is_ok());
  }

  #[test]
  fn editing_the_due_time_clears_the_reminder_but_ticking_does_not() {
    let tasks = added(&[], "p1", "renew proxy", Some(100), 1);
    let id = tasks[0].id.clone();
    let reminded = with_reminded(&tasks, std::slice::from_ref(&id));
    assert!(reminded[0].reminded);

    let ticked = with_patched(
      &reminded,
      &id,
      &TaskPatch {
        done: Some(true),
        ..TaskPatch::default()
      },
      5,
    )
    .unwrap();
    assert!(ticked[0].done && ticked[0].reminded);
    assert_eq!(ticked[0].updated_at, 5);

    let same_due = with_patched(
      &ticked,
      &id,
      &TaskPatch {
        due_at: Some(100),
        ..TaskPatch::default()
      },
      6,
    )
    .unwrap();
    assert!(same_due[0].reminded);

    let moved = with_patched(
      &ticked,
      &id,
      &TaskPatch {
        due_at: Some(200),
        ..TaskPatch::default()
      },
      6,
    )
    .unwrap();
    assert_eq!(moved[0].due_at, Some(200));
    assert!(!moved[0].reminded);

    let cleared = with_patched(
      &moved,
      &id,
      &TaskPatch {
        clear_due: true,
        ..TaskPatch::default()
      },
      7,
    )
    .unwrap();
    assert_eq!(cleared[0].due_at, None);
  }

  #[test]
  fn editing_an_unknown_task_or_with_a_bad_title_fails_without_changes() {
    let tasks = added(&[], "p1", "a", None, 1);
    let missing = with_patched(&tasks, "nope", &TaskPatch::default(), 2).unwrap_err();
    assert!(missing.contains("PROFILE_TASK_NOT_FOUND"));
    let bad = with_patched(
      &tasks,
      &tasks[0].id,
      &TaskPatch {
        title: Some("  ".into()),
        ..TaskPatch::default()
      },
      2,
    )
    .unwrap_err();
    assert!(bad.contains("PROFILE_TASK_TITLE_EMPTY"));
  }

  #[test]
  fn display_order_puts_open_dated_tasks_first_then_undated_then_done() {
    let mut tasks = Vec::new();
    tasks = added(&tasks, "p", "undated", None, 1);
    tasks = added(&tasks, "p", "late", Some(300), 2);
    tasks = added(&tasks, "p", "soon", Some(100), 3);
    tasks = added(&tasks, "p", "finished", Some(50), 4);
    let finished = tasks[3].id.clone();
    tasks = with_patched(
      &tasks,
      &finished,
      &TaskPatch {
        done: Some(true),
        ..TaskPatch::default()
      },
      5,
    )
    .unwrap();
    sort_for_display(&mut tasks);
    let titles: Vec<&str> = tasks.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["soon", "late", "undated", "finished"]);
  }

  #[test]
  fn the_file_round_trips_through_the_data_directory() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = crate::app_dirs::set_test_data_dir(dir.path().to_path_buf());
    assert!(list_profile_tasks(None).unwrap().is_empty());

    let task = add_profile_task("p1".into(), "ship it".into(), Some(42)).unwrap();
    add_profile_task("p2".into(), "other".into(), None).unwrap();
    assert_eq!(list_profile_tasks(Some("p1".into())).unwrap().len(), 1);
    assert_eq!(list_profile_tasks(None).unwrap().len(), 2);

    mark_profile_tasks_reminded(vec![task.id.clone()]).unwrap();
    assert!(list_profile_tasks(Some("p1".into())).unwrap()[0].reminded);

    delete_profile_task(task.id.clone()).unwrap();
    assert!(delete_profile_task(task.id)
      .unwrap_err()
      .contains("PROFILE_TASK_NOT_FOUND"));
    assert_eq!(delete_profile_tasks("p2".into()).unwrap(), 1);
    assert!(list_profile_tasks(None).unwrap().is_empty());
  }

  #[test]
  fn a_damaged_file_is_reported_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = crate::app_dirs::set_test_data_dir(dir.path().to_path_buf());
    let path = file_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{not json").unwrap();
    let err = add_profile_task("p".into(), "x".into(), None).unwrap_err();
    assert!(err.contains("PROFILE_TASKS_LOAD_FAILED"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{not json");
  }
}
