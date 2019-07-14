extern crate libc;
extern crate region;

use std::marker;
use std::mem;
use std::slice;
use std::alloc::{Layout, Global, Alloc};
use exception::Exception;

#[allow(dead_code)]
pub struct CodeSpace {
    pub(crate) inner: *mut u8,
    cap: usize,
    len: usize,
}

impl CodeSpace {
    /// Allocate memory.
    pub fn new(num_pages: usize) -> CodeSpace {
        let ptr: *mut u8;
        let page_size = region::page::size();
        let size = num_pages * page_size;
        unsafe {
            let layout = Layout::from_size_align_unchecked(size, page_size);
            ptr = match Global.alloc(layout) {
                Ok(p) => { p.as_ptr() }
                Err(e) => { panic!("Cannot allocate code space: {}", e) }
            };
            match region::protect(ptr, size, region::Protection::ReadWriteExecute) {
                Ok(_) => {
                    // Do nothing.
                }
                Err(e) => { panic!("Cannot allocate code space: {}", e) }
            }
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            libc::memset(ptr as *mut libc::c_void, 0xc3, size); // prepopulate with 'RET'
            #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
            libc::memset(ptr as *mut libc::c_void, 0x00, size);
        }
        CodeSpace {
            inner: ptr,
            cap: size,
            len: 0,
        }
    }
}

impl Memory for CodeSpace {
    fn start(&self) -> usize {
        unsafe { self.inner.offset(0) as usize }
    }

    fn limit(&self) -> usize {
        unsafe { self.inner.offset(self.cap as isize) as usize }
    }

    fn here(&mut self) -> usize {
        unsafe { self.inner.offset(self.len as isize) as usize }
    }

    fn set_here(&mut self, pos: usize) -> Result<(), Exception> {
        // here is allowed to be 1 place after the last memory address.
        if self.start() <= pos && pos <= self.limit() {
            let len = pos as isize - self.start() as isize;
            self.len = len as usize;
            Ok(())
        } else {
            Err(Exception::InvalidMemoryAddress)
        }
    }
}

pub struct SystemVariables {
    null: isize,
    base: isize,
}

impl SystemVariables {
    pub fn base_addr(&self) -> usize {
        &self.base as *const _ as usize
    }
}

#[allow(dead_code)]
pub struct DataSpace {
    pub inner: *mut u8,
    cap: usize,
    len: usize,
    marker: marker::PhantomData<SystemVariables>,
}

impl DataSpace {
    pub fn new(num_pages: usize) -> DataSpace {
        let ptr: *mut u8;
        let page_size = region::page::size();
        let size = num_pages * page_size;
        unsafe {
            let layout = Layout::from_size_align_unchecked(size, page_size);
            ptr = match Global.alloc(layout) {
                Ok(p) => { p.as_ptr() }
                Err(e) => { panic!("Cannot allocate data space: {}", e) }
            };
            match region::protect(ptr, size, region::Protection::ReadWrite) {
                Ok(_) => {
                    // Do nothing.
                }
                Err(e) => { panic!("Cannot allocate data space: {}", e) }
            }
            libc::memset(ptr as *mut libc::c_void, 0x00, size);
        }
        let mut result = DataSpace {
            inner: ptr,
            cap: size,
            len: mem::size_of::<SystemVariables>(),
            marker: marker::PhantomData,
        };
        result.system_variables_mut().null = 0;
        result.system_variables_mut().base = 10;
        result
    }

    // Getter

    pub fn system_variables(&self) -> &SystemVariables {
        unsafe { &*(self.inner.offset(0) as *const SystemVariables) }
    }

    pub fn system_variables_mut(&mut self) -> &mut SystemVariables {
        unsafe { &mut *(self.inner.offset(0) as *mut SystemVariables) }
    }
}

impl Memory for DataSpace {
    fn start(&self) -> usize {
        unsafe { self.inner.offset(0) as usize }
    }

    fn limit(&self) -> usize {
        unsafe { self.inner.offset(self.cap as isize) as usize }
    }

    fn here(&mut self) -> usize {
        unsafe { self.inner.offset(self.len as isize) as usize }
    }

    fn set_here(&mut self, pos: usize) -> Result<(), Exception> {
        // here is allowed to be 1 place after the last memory address.
        if self.start() <= pos && pos <= self.limit() {
            let len = pos as isize - self.start() as isize;
            self.len = len as usize;
            Ok(())
        } else {
            Err(Exception::InvalidMemoryAddress)
        }
    }
}

pub(crate) trait Memory {
    /// Start address
    fn start(&self) -> usize;

    /// Upper limit of address
    fn limit(&self) -> usize;

    /// Does memory contains addresss `pos`?
    ///
    /// True if self.start() <= pos < self.limit()
    fn has(&self, pos: usize) -> bool {
        self.start() <= pos && pos < self.limit()
    }

    /// Next free space
    fn here(&mut self) -> usize;

    /// Set next free space.
    fn set_here(&mut self, pos: usize) -> Result<(), Exception>;

    unsafe fn get_u8(&self, addr: usize) -> u8 {
        *(addr as *mut u8)
    }

    unsafe fn get_usize(&self, addr: usize) -> usize {
        *(addr as *mut usize)
    }

    unsafe fn get_isize(&self, addr: usize) -> isize {
        *(addr as *mut isize)
    }

    unsafe fn get_f64(&self, addr: usize) -> f64 {
        *(addr as *mut f64)
    }

    unsafe fn get_str(&self, addr: usize) -> &str {
        let len = self.get_usize(addr);
        let a = addr + mem::size_of::<usize>();
        self.str_from_raw_parts(a, len)
    }

    unsafe fn str_from_raw_parts(&self, addr: usize, len: usize) -> &str {
        mem::transmute(slice::from_raw_parts::<u8>(addr as *mut u8, len))
    }

    // Basic operations

    unsafe fn put_u8(&mut self, v: u8, pos: usize) {
        *(pos as *mut u8) = v;
    }

    #[allow(dead_code)]
    fn compile_u8(&mut self, v: u8) {
        let here = self.here();
        if here < self.limit() {
            unsafe {
                self.put_u8(v, here);
            }
            self.allot(mem::size_of::<u8>() as isize);
        } else {
            panic!("Error: compile_u8 while space is full.");
        }
    }

    unsafe fn put_usize(&mut self, v: usize, pos: usize) {
        *(pos as *mut usize) = v;
    }

    fn compile_usize(&mut self, v: usize) {
        let here = self.here();
        if here + mem::size_of::<usize>() <= self.limit() {
            unsafe {
                self.put_usize(v, here);
            }
            self.allot(mem::size_of::<usize>() as isize);
        } else {
            panic!("Error: compile_usize while space is full.");
        }
    }

    fn compile_relative(&mut self, f: usize) {
        let there = self.here() + mem::size_of::<usize>();
        let diff = f.wrapping_sub(there) as usize;
        self.compile_usize(diff);
    }

    unsafe fn put_isize(&mut self, v: isize, pos: usize) {
        *(pos as *mut isize) = v;
    }

    fn compile_isize(&mut self, v: isize) {
        let here = self.here();
        if here + mem::size_of::<isize>() <= self.limit() {
            unsafe {
                self.put_isize(v, here);
            }
            self.allot(mem::size_of::<isize>() as isize);
        } else {
            panic!("Error: compile_isize while space is full.");
        }
    }
    unsafe fn put_f64(&mut self, v: f64, pos: usize) {
        *(pos as *mut f64) = v;
    }

    fn compile_f64(&mut self, v: f64) {
        let here = self.here();
        if here + mem::size_of::<f64>() <= self.limit() {
            unsafe {
                self.put_f64(v, here);
            }
            self.allot(mem::size_of::<f64>() as isize);
        } else {
            panic!("Error: compile_f64 while space is full.");
        }
    }

    // Put counted string.
    fn put_cstr(&mut self, s: &str, pos: usize) {
        let bytes = s.as_bytes();
        let len = bytes.len().min(255);
        if pos + len + mem::size_of::<usize>() <= self.limit() {
            let mut p = pos;
            unsafe{ *(p as *mut u8) = len as u8; }
            for byte in &bytes[0..len] {
                p += 1;
                unsafe{ *(p as *mut u8) = *byte; }
            }
        } else {
            panic!("Error: put_cstr while space is full.");
        }

    }

    fn compile_str(&mut self, s: &str) -> usize {
        let bytes = s.as_bytes();
        let here = self.here();
        let len = bytes.len();
        if here + len + mem::size_of::<usize>() <= self.limit() {
            self.compile_usize(len);
            for byte in bytes {
                self.compile_u8(*byte);
            }
            here
        } else {
            panic!("Error: compile_str while space is full.");
        }
    }

    /// First aligned address greater than or equal to `pos`.
    fn aligned(pos: usize) -> usize {
        let align = mem::align_of::<isize>();
        (pos + align - 1) & align.wrapping_neg()
    }

    /// If the data-space pointer is not aligned, reserve enough space to align it.
    fn align(&mut self) {
        let here = self.here();
        self.set_here(Self::aligned(here));
    }

    /// First float-aligned address greater than or equal to `pos`.
    fn aligned_f64(pos: usize) -> usize {
        let align = mem::align_of::<f64>();
        (pos + align - 1) & align.wrapping_neg()
    }

    /// If the data-space pointer is not float-aligned, reserve enough space to align it.
    fn align_f64(&mut self) {
        let here = self.here();
        self.set_here(Self::aligned_f64(here));
    }

    /// First address aligned to 16-byte boundary greater than or equal to `pos`.
    fn aligned_16bytes(pos: usize) -> usize {
        let align = 16;
        (pos + align - 1) & align.wrapping_neg()
    }

    /// If the space pointer is not aligned to 16-byte boundary, reserve enough space to align it.
    fn align_16bytes(&mut self) {
        let here = self.here();
        self.set_here(Self::aligned_16bytes(here));
    }

    fn allot(&mut self, v: isize) {
        let here = (self.here() as isize + v) as usize;
        self.set_here(here);
    }

    fn truncate(&mut self, pos: usize) {
        self.set_here(pos);
    }
}
