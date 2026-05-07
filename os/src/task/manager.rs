//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::config::MAX_TIME;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        // self.ready_queue.pop_front()
        let mut min_stride = usize::MAX;
        let mut next_task_id = None;

        for (id,task) in self.ready_queue.iter().enumerate(){
            if task.inner_exclusive_access().stride < min_stride{
                min_stride = task.inner_exclusive_access().stride; 
                next_task_id = Some(id);
            }
        }
        
        match next_task_id{
            Some(value)=>{
                let mut task = self.ready_queue.remove(value);
                let mut inner = task.as_mut().unwrap().inner_exclusive_access();
                inner.stride += MAX_TIME / (inner.priority as usize);
                drop(inner);
                return task;
            },
            None=>{
                return None;
            }
        }

    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    //trace!("kernel: TaskManager::add_task");
    TASK_MANAGER.exclusive_access().add(task);
}

/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}
