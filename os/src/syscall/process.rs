//! Process management syscalls

use alloc::sync::Arc;

use crate::{
    config::PAGE_SIZE, loader::get_app_data_by_name, mm::{MapPermission, PTEFlags, PhysPageNum, VirtAddr, VirtPageNum, translated_refmut, translated_str}, task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next,
    }
    
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!("kernel::pid[{}] sys_waitpid [{}]", current_task().unwrap().pid.0, pid);
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}
/// wrap the mmmap process and help user to translate
fn user_translate(v:usize)->Option<(PhysPageNum,usize,PTEFlags)>{
    let va :VirtAddr = VirtAddr::from(v);

    let offset = va.page_offset();
    let vpn:VirtPageNum = va.floor();
    let current_task = current_task().unwrap();
    let inner = current_task.inner_exclusive_access();
    let memory_set = &inner.memory_set;
    let pte = match memory_set.translate(vpn){
        Some(value)=>{
            value
        },
        None=>{
            return None;
        }
    };
    if !pte.is_valid(){
        return None;
    }
    Some((pte.ppn(),offset,pte.flags()))
}

fn copy_to_user(dst:usize,src:&[u8])->Result<(),()>{
    let mut written = 0usize;

    while written<src.len(){
        let cur_va = dst.checked_add(written).ok_or(())?;
        let (ppn,offset,flags) = user_translate(cur_va).ok_or(())?;

        if !flags.contains(PTEFlags::U) || !flags.contains(PTEFlags::W){
            return Err(());
        }

        let n = core::cmp::min(PAGE_SIZE-offset,src.len()-written);
        ppn.get_bytes_array()[offset..offset+n].copy_from_slice(&src[written..written+n]);
        written +=n;

    }

    Ok(())

}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );

    if ts.is_null(){
        return -1;
    }
    let us = crate::timer::get_time_us();

    let tv = TimeVal{
        sec:us/1_000_000,
        usec:us%1_000_000
    };
    let bytes = unsafe{
        core::slice::from_raw_parts(&tv as *const TimeVal as *const u8,core::mem::size_of::<TimeVal>())
    };
    copy_to_user(ts as usize,bytes).map(|_| 0).unwrap_or(-1)
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if len ==0{
        return -1;
    }
    if prot ==0 || (prot&!0x7)!=0{
        return -1;
    }
    if start % PAGE_SIZE !=0{
        return -1;
    }

    let end = match start.checked_add(len){
        Some(value)=>value,
        None=>{
            return -1;
        }
    };
    let mid_prot = prot as u8;
    let mut perm = MapPermission::U;
    if (mid_prot&0b001)!=0{
        perm |= MapPermission::R;
    }
    if (mid_prot&0b010)!=0{
        perm |= MapPermission::W;
    }
    if (mid_prot&0b100)!=0{
        perm |= MapPermission::X;
    }

    let start_va :VirtAddr = start.into();
    let end_va :VirtAddr = end.into();
    let start_vpn = start_va.floor().0;
    let end_vpn = end_va.ceil().0;

    let current_task = current_task().unwrap();
    let memory_set = &mut current_task.inner_exclusive_access().memory_set;
    
    for vpn in start_vpn..end_vpn{
        if let Some(pte) = memory_set.translate(vpn.into()){
            if pte.is_valid(){
                return -1;
            }
        }
    }
    memory_set.insert_framed_area(start_va, end_va, perm);
    return 0;
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if len ==0{
        return -1;
    }
    if start%PAGE_SIZE !=0 || len%PAGE_SIZE!=0{
        return -1;
    }
    let start_va:VirtAddr = start.into();
    let end = match start.checked_add(len){
        Some(value)=>value,
        None=>{
            return -1;
        }
    };
    let end_va :VirtAddr = end.into();
    let start_vpn = start_va.floor().0;
    let end_vpn = end_va.ceil().0;
    
    let current_task = current_task().unwrap();

    let memory_set = &mut current_task.inner_exclusive_access().memory_set;

    memory_set.ummap(start_vpn.into(),end_vpn.into()).map(|_| 0 ).unwrap_or(-1)

}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // let pid = sys_fork();

    let token = current_user_token();
    let path = translated_str(token,path);
    if let Some(data) = get_app_data_by_name(path.as_str()){

        let task = current_task().unwrap();
        let new_task = task.fork();
        let new_pid = new_task.pid.0;
        new_task.exec(data);
        add_task(new_task);
        return new_pid as isize;
    }else{
        return -1;
    }
    // if pid ==0{
    //     let ret = sys_exec(path);

    //     sys_exit(ret as i32)
    // }
    // pid
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if prio < 2{
        return -1;
    }

    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();

    if inner.priority==0 || inner.priority ==prio{
        inner.priority = prio;
    }else
    {
        inner.stride = inner.stride * inner.stride /(prio as usize);
        inner.priority = prio;
    }
    drop(inner);
    return prio;
    // task.set_program_priority(prio)
}
