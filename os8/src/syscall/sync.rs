use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;

use super::process;

pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}

// LAB5 HINT: you might need to maintain data structures used for deadlock detection
// during sys_mutex_* and sys_semaphore_* syscalls
pub fn sys_mutex_create(blocking: bool) -> isize {
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        process_inner.mutex_available[id]=1;
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        let id=process_inner.mutex_list.len()-1;
        process_inner.mutex_available[id]=1;
        process_inner.mutex_list.len() as isize - 1
    }
}

// LAB5 HINT: Return -0xDEAD if deadlock is detected
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    if process_inner.enable_dead_lock_dect{
        if process_inner.mutex_available[mutex_id]==0{
            return -0xDEAD as isize;
        }
        process_inner.mutex_available[mutex_id]-=1;
    }
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.lock();
    0
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.unlock();
    0
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        process_inner.semaphore_available[id]=res_count as i32;
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        let id=process_inner.semaphore_list.len()-1;
        process_inner.semaphore_available[id]=res_count as i32;
        process_inner.semaphore_list.len() - 1
    };
    id as isize
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let tid=current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    process_inner.semaphore_allocation[tid][sem_id]-=1;
    process_inner.semaphore_available[sem_id]+=1;//finish应该在哪里修改？？？
    //println!("thread {} up {}",tid,sem_id);
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    sem.up();
    0
}

// LAB5 HINT: Return -0xDEAD if deadlock is detected
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let tid=current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    process_inner.semaphore_need[tid][sem_id]+=1;
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let len=process_inner.semaphore_list.len();
    // println!("semaphore before down is ");
    // for i in 0..len{
    //     print!("{} ",process_inner.semaphore_available[i]);
    // }
    // println!("");
    // println!("thread {} down {}",tid,sem_id);
    drop(process_inner);
    if !check_deadlock_sem(){
        return -0xDEAD as isize;
    }
    sem.down();
    let mut process_inner = process.inner_exclusive_access();
    let tid=current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
    process_inner.semaphore_allocation[tid][sem_id]+=1;
    process_inner.semaphore_available[sem_id]-=1;
    process_inner.semaphore_need[tid][sem_id]-=1;
    // println!("task {} allocation is",tid);
    // for i in 0..process_inner.semaphore_list.len(){
    //     print!("{} ",process_inner.semaphore_allocation[tid][i]);
    // }
    // println!("");
    drop(process_inner);
    0
}

pub fn sys_condvar_create(_arg: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    0
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    condvar.wait(mutex);
    0
}

// LAB5 YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(_enabled: usize) -> isize {
    let process=current_process();
    let mut process_inner=process.inner_exclusive_access();
    process_inner.enable_dead_lock_dect=true;
    drop(process_inner);
    0
}

pub fn check_deadlock_sem()->bool{
    let process=current_process();
    let process_inner=process.inner_exclusive_access();
    let thread_num=process_inner.tasks.len();
    let mut work=process_inner.semaphore_available;
    let mut flag=true;
    let mut finish=process_inner.semaphore_finish;
    let mut can_finish=true;
    let mut check_flag=false;
    while flag{
        flag=false;
        for i in 0..thread_num{
            if finish[i]==false{
                can_finish=true;
                flag=true;
                for j in 0..process_inner.semaphore_list.len(){
                    if process_inner.semaphore_need[i][j]>work[j]{
                        can_finish=false;
                        break;
                    }
                }
                if can_finish{
                    check_flag=true;
                    for j in 0..process_inner.semaphore_list.len(){
                        work[j]+=process_inner.semaphore_allocation[i][j];
                    }
                    finish[i]=true;
                }
            }
        }
        if !check_flag{
            break;
        }
        check_flag=false;
    }
    for i in 0..thread_num{
        if finish[i]==false{
            return false;
        }
    }
    return true;
}