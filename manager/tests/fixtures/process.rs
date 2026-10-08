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
        "owner-start-failure" => owner_start_failure(),
        "owner-exit" => {
            use vcxsrv_wsl_manager::{
                instance::InstanceGuard,
                owner::{ServerBinding, ServerOwner},
                storage::ProtectedDirectory,
            };
            let path = std::path::PathBuf::from(args.next().unwrap());
            let scope = args.next().unwrap();
            let abrupt = args.next().unwrap() == "abrupt";
            let guard = std::sync::Arc::new(InstanceGuard::acquire(&scope).unwrap());
            let store = ProtectedDirectory::open_or_create(&path, guard).unwrap();
            drop(store.create_new("authority", b"fixture").unwrap());
            let binding = ServerBinding::new(
                &store,
                std::env::current_exe().unwrap(),
                "NixOS".into(),
                "Empty".into(),
                1,
                "authority",
            )
            .unwrap();
            let owner = ServerOwner::spawn(&store, &binding, &["sleep".into()]).unwrap();
            assert!(owner.is_alive().unwrap());
            if abrupt {
                std::process::exit(0)
            }
            // Returning exercises ordinary Rust drops; process::exit skips them.
        }
        "job-report" => {
            use windows_sys::Win32::{
                Foundation::*,
                System::{JobObjects::*, Threading::*},
            };
            unsafe {
                let mut present = 0;
                IsProcessInJob(GetCurrentProcess(), std::ptr::null_mut(), &mut present);
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                let ok = QueryInformationJobObject(
                    std::ptr::null_mut(),
                    JobObjectExtendedLimitInformation,
                    (&mut info as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32,
                    std::ptr::null_mut(),
                );
                std::fs::write(
                    args.next().unwrap(),
                    format!(
                        "job={present}; query={ok}; error={}; flags={:x}",
                        GetLastError(),
                        info.BasicLimitInformation.LimitFlags
                    ),
                )
                .unwrap();
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        "instance" => {
            let scope = args.next().unwrap();
            match vcxsrv_wsl_manager::instance::InstanceGuard::acquire(&scope) {
                Ok(_guard) => print!("acquired"),
                Err(_) => print!("denied"),
            }
        }
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

fn owner_start_failure() {
    use vcxsrv_wsl_manager::{
        instance::InstanceGuard,
        owner::{ServerBinding, ServerOwner},
        storage::{ProtectedDirectory, StorageError},
    };
    let scope = format!("fixture-failure-{}", std::process::id());
    let path = std::env::temp_dir().join(&scope);
    let guard = std::sync::Arc::new(InstanceGuard::acquire(&scope).unwrap());
    let store = ProtectedDirectory::open_or_create(&path, guard).unwrap();
    drop(store.create_new("authority", b"fixture").unwrap());
    std::fs::copy(std::env::current_exe().unwrap(), path.join("server.exe")).unwrap();
    let binding = ServerBinding::new(
        &store,
        path.join("server.exe"),
        "NixOS".into(),
        "Empty".into(),
        1,
        "authority",
    )
    .unwrap();
    assert!(ServerOwner::spawn(
        &store,
        &binding,
        &["job-report".into(), path.join("ran.txt").into_os_string()]
    )
    .is_err());
    assert!(!path.join("ran.txt").exists());
    assert!(matches!(
        store.read("ownership.json", 16384),
        Err(StorageError::Missing)
    ));
    drop(binding);
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
    print!("safe-failure");
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
