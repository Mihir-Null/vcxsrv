use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    time::Duration,
};
fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap();
    match mode.as_str() {
        "argv" => {
            for arg in args {
                let bytes = arg.as_bytes();
                std::io::stdout()
                    .write_all(&(bytes.len() as u32).to_le_bytes())
                    .unwrap();
                std::io::stdout().write_all(bytes).unwrap();
            }
        }
        "sleep" | "no-stdin" => std::thread::sleep(Duration::from_secs(60)),
        "stdin" => {
            let mut b = Vec::new();
            std::io::stdin().read_to_end(&mut b).unwrap();
            print!("{}", b.len());
        }
        "secret-fail" => {
            let mut b = Vec::new();
            std::io::stdin().read_to_end(&mut b).unwrap();
            std::io::stderr().write_all(&b).unwrap();
            std::process::exit(23);
        }
        "exit" => std::process::exit(23),
        "pipe-private" => pipe_private(),
        "inheritance" => {
            use windows_sys::Win32::Foundation::{GetHandleInformation, HANDLE};
            let handle = args.next().unwrap().parse::<usize>().unwrap() as HANDLE;
            let mut flags = 0;
            let inherited = unsafe { GetHandleInformation(handle, &mut flags) } != 0;
            print!("{}", if inherited { "inherited" } else { "excluded" });
        }
        "flood" => {
            let a = std::thread::spawn(|| {
                let _ = std::io::stderr().write_all(&vec![b'e'; 2_000_000]);
            });
            let _ = std::io::stdout().write_all(&vec![b'o'; 2_000_000]);
            let _ = a.join();
        }
        "descendant" => {
            let child = Command::new(std::env::current_exe().unwrap())
                .arg("sleep")
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            if let Some(path) = args.next() {
                std::fs::write(path, child.id().to_string()).unwrap();
            }
            println!("{}", child.id());
            std::mem::forget(child);
        }
        _ => panic!("unknown fixture mode"),
    }
}

fn pipe_private() {
    use std::ptr::{addr_of, null_mut};
    use windows_sys::Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::FILE_ALL_ACCESS,
        System::{Console::*, Threading::*},
    };
    unsafe {
        let mut token = null_mut();
        assert_ne!(
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token),
            0
        );
        let mut n = 0;
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut n);
        let mut data = vec![0usize; (n as usize).div_ceil(std::mem::size_of::<usize>())];
        assert_ne!(
            GetTokenInformation(token, TokenUser, data.as_mut_ptr().cast(), n, &mut n),
            0
        );
        CloseHandle(token);
        let sid = (*(data.as_ptr().cast::<TOKEN_USER>())).User.Sid;
        let mut dacl = null_mut();
        let mut sd = null_mut();
        if GetSecurityInfo(
            GetStdHandle(STD_INPUT_HANDLE),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut sd,
        ) != 0
        {
            print!("unsafe");
            return;
        }
        let mut control = 0;
        let mut revision = 0;
        assert_ne!(
            GetSecurityDescriptorControl(sd, &mut control, &mut revision),
            0
        );
        let private = if dacl.is_null() || (*dacl).AceCount != 1 {
            false
        } else {
            let mut ace = null_mut();
            assert_ne!(GetAce(dacl, 0, &mut ace), 0);
            let ace = ace.cast::<ACCESS_ALLOWED_ACE>();
            (*ace).Header.AceType == 0
                && ((*ace).Mask == GENERIC_ALL || (*ace).Mask == FILE_ALL_ACCESS)
                && EqualSid(sid, addr_of!((*ace).SidStart).cast_mut().cast()) != 0
                && control & SE_DACL_PROTECTED != 0
        };
        LocalFree(sd);
        print!("{}", if private { "private" } else { "unsafe" });
    }
}
