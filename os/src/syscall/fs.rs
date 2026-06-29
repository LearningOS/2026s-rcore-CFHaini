//! File and filesystem-related syscalls
use crate::fs::{OpenFlags, SharedInode, Stat, StatMode, check_sharedinode, open_file, open_file_get_inode_id};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::syscall::process::copy_to_user;
use crate::task::{current_task, current_user_token};
use crate::fs::{link_modify,unlink_modify};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");

        // 在持有可能阻塞的锁时（如文件系统锁时），要绝对避免直接访问用户态内存或进行可能导致内存的回收分配
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(fd: usize, st: *mut Stat) -> isize {
    trace!(
        "kernel:pid[{}] sys_fstat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();

    if fd >= inner.fd_table.len(){
        return -1;
    }
    if inner.fd_table[fd].is_none(){
        return -1;
    }
    let (block_id,block_offset,inode_id) = inner.fd_table[fd].as_ref().unwrap().return_block_info();
    let file_type = match inner.fd_table[fd].as_ref().unwrap().file_type(){
        0 =>{
            StatMode::FILE
        },
        1=>{
            StatMode::DIR
        },
        _=>{
            StatMode::NULL
        }
    };
    
    let data = SharedInode::new(block_id,block_offset);
    let nlink = check_sharedinode(data).unwrap_or(0);
    let final_data = Stat::new(inode_id as u64,file_type,nlink as u32);
    let src = unsafe{
        core::slice::from_raw_parts(&final_data as *const Stat as *const u8,core::mem::size_of::<Stat>())
    };
    
    drop(inner);
    copy_to_user(st as usize, src).map(|_| 0 ).unwrap_or(-1)
}

/// YOUR JOB: Implement linkat.
/// 为了方便，不考虑新文件路径已经存在的情况(属于未定义的行为)。除非出现新旧名字一致的情况，此时需要返回-1
/// 返回值：如果出现了错误则返回-1，否则返回0

pub fn sys_linkat(old_name: *const u8, new_name: *const u8) -> isize {
    // 实现思路：
    // 1.首先讲old_name 和 new_name通过地址转换为 String类型，判断其是否相等
    // 若相等，则返回-1
    // 若不相等，继续以下流程
    // 2. 
    trace!(
        "kernel:pid[{}] sys_linkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    // let task = current_task().unwrap();
    let token = current_user_token();
    let old_path = translated_str(token,old_name); //找到String类型的字符串名称
    let new_path = translated_str(token,new_name);

    if old_path != new_path{
        if let Some(inode_id) = open_file_get_inode_id(&old_path){
            link_modify(inode_id,&new_path);
            0isize
        }else{
            -1
        }
        
    }else{
        -1
    }
}

/// YOUR JOB: Implement unlinkat.
/// 说明：注意考虑使用unlink彻底删除文件的情况，此时需要回收inode以及它对应的数据块
pub fn sys_unlinkat(name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let path = translated_str(token, name);
    if let Some(inode_id) = open_file_get_inode_id(&path){
        unlink_modify(inode_id,&path);
        0
    }else{
        -1
    }

}
