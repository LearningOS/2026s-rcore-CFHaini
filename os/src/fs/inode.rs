//! `Arc<Inode>` -> `OSInodeInner`: In order to open files concurrently
//! we need to wrap `Inode` into `Arc`,but `Mutex` in `Inode` prevents
//! file systems from being accessed simultaneously
//!
//! `UPSafeCell<OSInodeInner>` -> `OSInode`: for static `ROOT_INODE`,we
//! need to wrap `OSInodeInner` into `UPSafeCell`
use super::File;
use crate::drivers::BLOCK_DEVICE;
use crate::mm::UserBuffer;
use crate::sync::UPSafeCell;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::*;
use easy_fs::{EasyFileSystem, Inode};
use lazy_static::*;
use alloc::collections::btree_map::{BTreeMap};
// use super::SharedInode;

/// inode in memory
/// A wrapper around a filesystem inode
/// to implement File trait atop
pub struct OSInode {
    readable: bool,
    writable: bool,
    inner: UPSafeCell<OSInodeInner>,
}
/// The OS inode inner in 'UPSafeCell'
pub struct OSInodeInner {
    offset: usize,
    inode: Arc<Inode>,
}

impl OSInode {
    /// create a new inode in memory
    pub fn new(readable: bool, writable: bool, inode: Arc<Inode>) -> Self {
        Self {
            readable,
            writable,
            inner: unsafe { UPSafeCell::new(OSInodeInner { offset: 0, inode }) },
        }
    }
    /// read all data from the inode
    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.exclusive_access();
        let mut buffer: Vec<u8> = Vec::with_capacity(512);
        buffer.resize(512, 0);
        let mut v: Vec<u8> = Vec::new();
        loop {
            let len = inner.inode.read_at(inner.offset, &mut buffer);
            if len == 0 {
                break;
            }
            inner.offset += len;
            v.extend_from_slice(&buffer[..len]);
        }
        v
    }
    /// get the block_id of OSInode
    pub fn get_block_id(&self)->usize{
        self.inner.exclusive_access().inode.block_id()
    }
    /// get the block_offset of OSInode
    pub fn get_block_offset(&self)->usize{
        self.inner.exclusive_access().inode.block_offset()
    }
}

lazy_static! {
    pub static ref ROOT_INODE: Arc<Inode> = {
        let efs = EasyFileSystem::open(BLOCK_DEVICE.clone());
        Arc::new(EasyFileSystem::root_inode(&efs))
    };
}
pub struct SharedInodemanager{
    pub inner:BTreeMap<SharedInode,usize>
}

/// SharedInode 结构体，记录（block_id,block_offset）信息
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct SharedInode{
    // 我自己设计的轻量级来记录inode里面的（block_id,block_offset）的数据结构
    block_id:usize,
    block_offset:usize,
}
/// 方法
impl SharedInode{
    /// new SharedInode
    pub fn new(block_id:usize,block_offset:usize)->Self{
        Self { block_id, block_offset}
    }
    
}
impl SharedInodemanager{
    pub fn new()->Self{
        Self { 
            inner:BTreeMap::new()
         }
    }
    pub fn insert(&mut self,data:SharedInode){
        if let Some(x) = self.inner.get_mut(&data){
            *x +=1;
        }
        else{
            self.inner.insert(data,1);
        }
    }
    pub fn check(&self,data:SharedInode)->Option<usize>{
        if let Some(x) = self.inner.get(&data){
            Some(*x)
        }else{
            None
        }
    }
    pub fn delete(&mut self,data:&SharedInode){
        self.inner.remove(data);
    }
    pub fn modify(&mut self,data:SharedInode)->Option<usize>{
        if let Some(x) = self.inner.get_mut(&data){
            *x -=1;
        }
        let x = *self.inner.get(&data).unwrap();

        if x==0{
            self.delete(&data);
        }
        Some(x)
    }
}

lazy_static!{
    pub static ref SHARE_INDOE_MANAGER:UPSafeCell<SharedInodemanager> = {
       unsafe{ UPSafeCell::new(SharedInodemanager::new()) }
    };
}

/// List all apps in the root directory
pub fn list_apps() {
    println!("/**** APPS ****");
    for app in ROOT_INODE.ls() {
        println!("{}", app);
    }
    println!("**************/");
}

bitflags! {
    ///  The flags argument to the open() system call is constructed by ORing together zero or more of the following values:
    pub struct OpenFlags: u32 {
        /// readyonly
        const RDONLY = 0;
        /// writeonly
        const WRONLY = 1 << 0;
        /// read and write
        const RDWR = 1 << 1;
        /// create new file
        const CREATE = 1 << 9;
        /// truncate file size to 0
        const TRUNC = 1 << 10;
    }
}

impl OpenFlags {
    /// Do not check validity for simplicity
    /// Return (readable, writable)
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)
        } else if self.contains(Self::WRONLY) {
            (false, true)
        } else {
            (true, true)
        }
    }
}

/// Open a file
pub fn open_file(name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    let (readable, writable) = flags.read_write();
    if flags.contains(OpenFlags::CREATE) {
        if let Some(inode) = ROOT_INODE.find(name) {
            // clear size
            inode.clear();
            // let data = SharedInode::new(inode.block_id(),inode.block_offset());
            // SHARE_INDOE_MANAGER.exclusive_access().insert(data);
            Some(Arc::new(OSInode::new(readable, writable, inode)))
        } else {
            // create file
            ROOT_INODE
                .create(name)
                .map(|inode| 
                    {   
                        //只需要在创建文件时才把(block_id,block_offset)加入管理器
                        let data = SharedInode::new(inode.block_id(),inode.block_offset());
                        SHARE_INDOE_MANAGER.exclusive_access().insert(data);
                        Arc::new(OSInode::new(readable, writable, inode))
                    })
        }
    } else {
        ROOT_INODE.find(name).map(|inode| {
            if flags.contains(OpenFlags::TRUNC) {
                inode.clear();
            }
            Arc::new(OSInode::new(readable, writable, inode))
        })
    }
}

impl File for OSInode {
    fn readable(&self) -> bool {
        self.readable
    }
    fn writable(&self) -> bool {
        self.writable
    }
    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_read_size = 0usize;
        for slice in buf.buffers.iter_mut() {
            let read_size = inner.inode.read_at(inner.offset, *slice);
            if read_size == 0 {
                break;
            }
            inner.offset += read_size;
            total_read_size += read_size;
        }
        total_read_size
    }
    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            let write_size = inner.inode.write_at(inner.offset, *slice);
            assert_eq!(write_size, slice.len());
            inner.offset += write_size;
            total_write_size += write_size;
        }
        total_write_size
    }
    fn return_block_info(&self)->(usize,usize,u32) {
        let block_id = self.get_block_id();
        let block_offset = self.get_block_offset();
        let inode_id = self.inner.exclusive_access().inode.get_inode_id_by_block(block_id as u32, block_offset);
        (block_id,block_offset,inode_id)

    }
    fn file_type(&self)->usize {
        match self.inner.exclusive_access().inode.get_file_type(){
            true=>{
                1usize
            },
            false=>{
                0usize
            }
        }
    }
}

/// open file get inode id
pub fn open_file_get_inode_id(name:&str)->Option<u32>{
    ROOT_INODE.find_return_inode_id(name) 
}
/// link modify
pub fn link_modify(inode_id:u32,name:&str){
   let (block_id,block_offset) = ROOT_INODE.inode_link_modify(inode_id,name);
   let data = SharedInode::new(block_id as usize,block_offset);
   SHARE_INDOE_MANAGER.exclusive_access().insert(data);
}
/// check sharedinode for system call fstat
pub fn check_sharedinode(data:SharedInode)->Option<usize>{
    SHARE_INDOE_MANAGER.exclusive_access().check(data)
}
/// unlink modify
pub fn unlink_modify(inode_id:u32,name:&str){
    let inode = ROOT_INODE.find(name).unwrap();
    let (block_id,block_offset) = ROOT_INODE.inode_unlink_modify(inode_id, name);
    let data = SharedInode::new(block_id as usize,block_offset);
    let cnt = SHARE_INDOE_MANAGER.exclusive_access().modify(data).unwrap();
    if cnt ==0{
        inode.clear();
    }

}