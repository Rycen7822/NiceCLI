use crate::tray_host::clear_tray_icon;
use serde_json::json;
use tauri::Emitter;

pub(crate) fn stop_managed_process() {
    if let Some(backend) = crate::IN_PROCESS_BACKEND.lock().take() {
        backend.stop();
    }

    let mut child = crate::PROCESS.lock().take();
    if let Some(child) = child.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }
    *crate::PROCESS_PID.lock() = None;
    *crate::CLI_PROXY_PASSWORD.lock() = None;
    *crate::MANAGED_BACKEND_INFO.lock() = None;
}

pub(crate) fn managed_process_running() -> bool {
    {
        let guard = crate::IN_PROCESS_BACKEND.lock();
        if let Some(backend) = guard.as_ref() {
            if backend.is_running() {
                return true;
            }
        }
    }
    if let Some(backend) = crate::IN_PROCESS_BACKEND.lock().take() {
        backend.stop();
    }

    let mut guard = crate::PROCESS.lock();
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(None) => {
                *crate::PROCESS_PID.lock() = Some(child.id());
                return true;
            }
            Ok(Some(_)) | Err(_) => {
                *guard = None;
                *crate::PROCESS_PID.lock() = None;
            }
        }
    }
    false
}

pub(crate) fn start_monitor(app: tauri::AppHandle) {
    let proc_ref = std::sync::Arc::clone(&crate::PROCESS);
    std::thread::spawn(move || loop {
        let mut remove = false;
        let mut exit_code: Option<i32> = None;
        {
            let mut guard = proc_ref.lock();
            if let Some(child) = guard.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        exit_code = status.code();
                        remove = true;
                    }
                    Ok(None) => {}
                    Err(_) => {
                        remove = true;
                    }
                }
            } else {
                break;
            }
        }
        if remove {
            *proc_ref.lock() = None;
            if let Some(code) = exit_code {
                println!("[NiceCLI][EXIT] process exited with code {}", code);
            } else {
                println!("[NiceCLI][EXIT] process closed (no exit code)");
            }
            if let Some(code) = exit_code {
                let _ = app.emit("process-exit-error", json!({"code": code}));
            } else {
                let _ = app.emit(
                    "process-closed",
                    json!({"message": "NiceCLI local runtime has closed"}),
                );
            }
            clear_tray_icon();
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1000));
    });
}

pub(crate) fn stop_process_internal() {
    stop_managed_process();
    println!("[NiceCLI][INFO] app closing - local runtime stopped");
}
