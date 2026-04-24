//! Process management syscalls


use crate::{config::{MAX_SYSCALL_NUM, PAGE_SIZE}, mm::{MapPermission, PTEFlags, PhysPageNum, VirtAddr, VirtPageNum}, task::{change_program_brk, current_task_memory_set, exit_current_and_run_next, suspend_current_and_run_next, syscall_count}};
use core::cmp::min;
#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}
/// wrap the mmap process and help user to translate
fn user_translate(v:usize)->Option<(PhysPageNum,usize,PTEFlags)>{
    let va:VirtAddr = v.into();
    let offset = va.page_offset();
    let vpn:VirtPageNum = va.floor();
    let memory_set = current_task_memory_set();
    let pte = match memory_set.translate(vpn){
        Some(value)=> value,
        None=>{
            return None;
        }
    };
    if !pte.is_valid(){
        return None;
    }
    Some((pte.ppn(),offset,pte.flags()))

}
/// copy data from kernel to user
fn copy_to_user(dst_va:usize,src:&[u8])->Result<(),()>{
    let mut written = 0usize;
    while written<src.len(){
        let cur_va = dst_va.checked_add(written).ok_or(())?;
        let (ppn,offset,flags) = user_translate(cur_va).ok_or(())?;

        if !flags.contains(PTEFlags::U) || !flags.contains(PTEFlags::W){
            return Err(());
        }

        let n = min(PAGE_SIZE - offset,src.len()-written);
        ppn.get_bytes_array()[offset..offset+n].copy_from_slice(&src[written..written+n]);
        written +=n;
    }

    Ok(())
}


/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    
    if ts.is_null(){
        return -1;
    }

    let us = crate::timer::get_time_us();
    
    let tv = TimeVal{
        sec:us/1_000_000,
        usec:us%1_000_000,
    };
    let bytes = unsafe{
        core::slice::from_raw_parts(&tv as *const TimeVal as *const u8,core::mem::size_of::<TimeVal>())
    };

    copy_to_user(ts as usize,bytes).map(|_| 0).unwrap_or(-1)
}


/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");

    match trace_request{
        0=>{

            let (ppn,offset,flags) = match user_translate(id){
                Some(value)=>{
                    value
                },
                None=>{
                    return -1;
                }
            };
            if !flags.contains(PTEFlags::U) || !flags.contains(PTEFlags::R){
                return -1;
            }

           return ppn.get_bytes_array()[offset] as isize;
        },
        1=>{
            let (ppn,offset,flags) = match user_translate(id){
                Some(value)=>{
                    value
                },
                None=>{
                    return -1;
                }
            };
            if !flags.contains(PTEFlags::U) || !flags.contains(PTEFlags::W)
            || (!flags.contains(PTEFlags::R) && flags.contains(PTEFlags::W)){
                return -1;
            }
            ppn.get_bytes_array()[offset] = data as u8;
            return 0 as isize;
        },
        2=>{
            if id <MAX_SYSCALL_NUM{
                return syscall_count(id) as isize;
            }
            else{
                return -1;
            }

        },
        _=>{
            return -1;
        }
    }

}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    if len == 0{
        return -1;
    }
    if start %PAGE_SIZE !=0{
        return -1;
    }
    if prot==0 || (prot&!0x7)!=0{
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
    
    if (mid_prot&0b001)!=0 {perm |=MapPermission::R;}
    if (mid_prot & 0b010) !=0 {perm |= MapPermission::W;}
    if (mid_prot &0b100)!=0 {perm |= MapPermission::X;}

    let start_va:VirtAddr = start.into();
    let end_va:VirtAddr = end.into();
    let start_vpn = start_va.floor().0;
    let end_vpn = end_va.ceil().0;

    let memory_set = current_task_memory_set();

    for vpn in start_vpn..end_vpn{
        if let Some(pte) = memory_set.translate(vpn.into()){
            if pte.is_valid(){
                return -1;
            }
        }
    }

    memory_set.insert_framed_area(start_va, end_va,perm);
    return 0;
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");

    if len==0{
        return -1;
    }
    if start %PAGE_SIZE!=0 || len%PAGE_SIZE!=0{
        return -1;
    }

    let start_va:VirtAddr = start.into();
    let end = match start.checked_add(len){
        Some(value)=>{
            value
        },
        None=>{
            return -1;
        }
    };
    let end_va:VirtAddr = end.into();

    let start_vpn = start_va.floor().0;
    let end_vpn = end_va.ceil().0;

    let memory_set = current_task_memory_set();

    // for vpn in start_vpn..end_vpn{
    //     if let Some(pte) = memory_set.translate(vpn.into()){
    //         if !pte.is_valid(){
    //             return -1;
    //         }
    //     }
    // }
    // for vpn in start_vpn..end_vpn{
    //     memory_set.ummap(vpn.into());
    // }
    // return 0;
    memory_set.ummap(start_vpn.into(),end_vpn.into()).map(|_| 0).unwrap_or(-1)

}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
