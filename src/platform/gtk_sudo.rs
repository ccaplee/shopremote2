// GTK를 사용한 sudo/su 암호 프롬프트 구현
// 참고: https://github.com/aarnt/qt-sudo
// 주의: 때때로 sudoers를 새로고침하기 위해 재부팅이 필요할 수 있음

use crate::lang::translate;
use gtk::{glib, prelude::*};
use hbb_common::{
    anyhow::{bail, Error},
    log,
    platform::linux::CMD_SH,
    ResultType,
};
use nix::{
    libc::{fcntl, kill},
    pty::{forkpty, ForkptyResult},
    sys::{
        signal::Signal,
        wait::{waitpid, WaitPidFlag},
    },
    unistd::{execvp, setsid, Pid},
};
use std::{
    ffi::CString,
    fs::File,
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
};

// 자식 프로세스 종료 코드
const EXIT_CODE: i32 = -1;

/// UI와 자식 프로세스 간의 메시지 타입
enum Message {
    /// 암호 입력 프롬프트: (에러 메시지, 사용자명 수정 가능 여부)
    PasswordPrompt((String, bool)),
    /// 입력된 암호: (사용자명, 암호)
    Password((String, String)),
    /// 에러 메시지 다이얼로그
    ErrorDialog(String),
    /// 사용자가 취소함
    Cancel,
    /// 프로세스 종료: 종료 코드
    Exit(i32),
}

/// sudo/su 명령을 실행하는 함수
/// 명령 행 인자를 기반으로 CLI 모드 또는 GUI 모드로 실행
/// shopremote2 서비스가 프로세스를 종료할 때를 대비한 안전장치 포함
pub fn run(cmds: Vec<&str>) -> ResultType<()> {
    // shopremote2 서비스가 `shopremote2 --` 프로세스 종료 처리
    let second_arg = std::env::args().nth(1).unwrap_or_default();
    // CLI 모드 판별: "--" 로 시작하지만 "--tray"나 "--no-server"는 아닌 경우
    let cmd_mode =
        second_arg.starts_with("--") && second_arg != "--tray" && second_arg != "--no-server";
    let mod_arg = if cmd_mode { "cmd" } else { "gui" };
    let mut args = vec!["-gtk-sudo", mod_arg];
    args.append(&mut cmds.clone());
    // 새 프로세스 생성 및 대기
    let mut child = crate::run_me(args)?;
    let exit_status = child.wait()?;
    if exit_status.success() {
        Ok(())
    } else {
        bail!("child exited with status: {:?}", exit_status);
    }
}

/// sudo/su 프로세스 실행 진입점
/// 세 번째 인자가 "cmd"이면 CLI 모드, 아니면 GUI 모드로 실행
pub fn exec() {
    let mut args = vec![];
    // 세 번째 인자부터 모든 명령 인자 수집
    for arg in std::env::args().skip(3) {
        args.push(arg);
    }
    let cmd_mode = std::env::args().nth(2) == Some("cmd".to_string());
    if cmd_mode {
        cmd(args);
    } else {
        ui(args);
    }
}

/// CLI 모드에서 sudo/su 명령 실행
/// PTY(Pseudo-Terminal)를 사용하여 암호 입력 처리
fn cmd(args: Vec<String>) {
    // 부모와 자식 프로세스를 위해 PTY 분리
    match unsafe { forkpty(None, None) } {
        Ok(forkpty_result) => match forkpty_result {
            ForkptyResult::Parent { child, master } => {
                // 부모 프로세스: 표준 입출력 중계
                if let Err(e) = cmd_parent(child, master) {
                    log::error!("Parent error: {:?}", e);
                    kill_child(child);
                    std::process::exit(EXIT_CODE);
                }
            }
            ForkptyResult::Child => {
                // 자식 프로세스: sudo/su 명령 실행
                if let Err(e) = child(None, args) {
                    log::error!("Child error: {:?}", e);
                    std::process::exit(EXIT_CODE);
                }
            }
        },
        Err(err) => {
            log::error!("forkpty error: {:?}", err);
            std::process::exit(EXIT_CODE);
        }
    }
}

/// GUI 모드에서 sudo/su 명령 실행
/// GTK 대화상자를 사용하여 암호 입력을 받음
/// 참고: https://docs.gtk.org/gtk4/ctor.Application.new.html
///       https://docs.gtk.org/gio/type_func.Application.id_is_valid.html
fn ui(args: Vec<String>) {
    // GTK 애플리케이션 생성
    let application = gtk::Application::new(None, Default::default());

    // UI와 자식 프로세스 간 양방향 통신 채널 생성
    let (tx_to_ui, rx_to_ui) = channel::<Message>();
    let (tx_from_ui, rx_from_ui) = channel::<Message>();

    // 채널을 Arc<Mutex<>>로 감싸서 여러 스레드에서 사용 가능하게 함
    let rx_to_ui = Arc::new(Mutex::new(rx_to_ui));
    let tx_from_ui = Arc::new(Mutex::new(tx_from_ui));

    let rx_to_ui_clone = rx_to_ui.clone();
    let tx_from_ui_clone = tx_from_ui.clone();

    // 현재 활성 사용자명을 공유 상태로 저장
    let username = Arc::new(Mutex::new(crate::platform::get_active_username()));
    let username_clone = username.clone();

    // 애플리케이션 활성화 시 UI 메시지 루프 설정
    application.connect_activate(glib::clone!(@weak application =>move |_| {
        let rx_to_ui = rx_to_ui_clone.clone();
        let tx_from_ui = tx_from_ui_clone.clone();
        // 마지막 입력된 암호 저장 (편의성)
        let last_password = Arc::new(Mutex::new(String::new()));
        let username = username_clone.clone();

        // 50ms 간격으로 메시지 확인
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            if let Ok(msg) = rx_to_ui.lock().unwrap().try_recv() {
                match msg {
                    // 암호 입력 프롬프트 메시지
                    Message::PasswordPrompt((err_msg, show_edit)) => {
                        let last_pwd = last_password.lock().unwrap().clone();
                        let username = username.lock().unwrap().clone();
                        if let Some((username, password)) = password_prompt(&username, &last_pwd, &err_msg, show_edit) {
                                *last_password.lock().unwrap() = password.clone();
                                if let Err(e) = tx_from_ui
                                    .lock()
                                    .unwrap()
                                    .send(Message::Password((username, password))) {
                                        error_dialog_and_exit(&format!("Channel error: {e:?}"), EXIT_CODE);
                                    }
                        } else {
                            if let Err(e) = tx_from_ui.lock().unwrap().send(Message::Cancel) {
                                error_dialog_and_exit(&format!("Channel error: {e:?}"), EXIT_CODE);
                            }
                        }
                    }
                    Message::ErrorDialog(err_msg) => {
                        error_dialog_and_exit(&err_msg, EXIT_CODE);
                    }
                    Message::Exit(code) => {
                        log::info!("Exit code: {}", code);
                        std::process::exit(code);
                    }
                    _ => {}
                }
            }
            glib::ControlFlow::Continue
        });
    }));

    // 자식 프로세스를 실행할 별도 스레드
    let tx_to_ui_clone = tx_to_ui.clone();
    std::thread::spawn(move || {
        let acitve_user = crate::platform::get_active_username();
        let mut initial_password = None;
        // root가 아닌 경우 초기 암호 입력
        if acitve_user != "root" {
            if let Err(e) = tx_to_ui_clone.send(Message::PasswordPrompt(("".to_string(), true))) {
                log::error!("Channel error: {e:?}");
                std::process::exit(EXIT_CODE);
            }
            match rx_from_ui.recv() {
                Ok(Message::Password((user, password))) => {
                    *username.lock().unwrap() = user;
                    initial_password = Some(password);
                }
                Ok(Message::Cancel) => {
                    log::info!("User canceled");
                    std::process::exit(EXIT_CODE);
                }
                _ => {
                    log::error!("Unexpected message");
                    std::process::exit(EXIT_CODE);
                }
            }
        }
        let username = username.lock().unwrap().clone();
        // 사용자 변경 필요 여부 판정
        let su_user = if username == acitve_user {
            None
        } else {
            Some(username)
        };
        match unsafe { forkpty(None, None) } {
            Ok(forkpty_result) => match forkpty_result {
                ForkptyResult::Parent { child, master } => {
                    if let Err(e) = ui_parent(
                        child,
                        master,
                        tx_to_ui_clone,
                        rx_from_ui,
                        su_user.is_some(),
                        initial_password,
                    ) {
                        log::error!("Parent error: {:?}", e);
                        kill_child(child);
                        std::process::exit(EXIT_CODE);
                    }
                }
                ForkptyResult::Child => {
                    if let Err(e) = child(su_user, args) {
                        log::error!("Child error: {:?}", e);
                        std::process::exit(EXIT_CODE);
                    }
                }
            },
            Err(err) => {
                log::error!("forkpty error: {:?}", err);
                if let Err(e) =
                    tx_to_ui.send(Message::ErrorDialog(format!("Forkpty error: {:?}", err)))
                {
                    log::error!("Channel error: {e:?}");
                    std::process::exit(EXIT_CODE);
                }
            }
        }
    });

    let _holder = application.hold();
    let args: Vec<&str> = vec![];
    application.run_with_args(&args);
    log::debug!("exit from gtk::Application::run_with_args");
    std::process::exit(EXIT_CODE);
}

/// CLI 모드 부모 프로세스 처리
/// PTY 마스터에서 자식 프로세스의 출력을 읽어 표준 출력으로 전달
/// 표준 입력에서 사용자 입력을 읽어 자식 프로세스로 전달
fn cmd_parent(child: Pid, master: OwnedFd) -> ResultType<()> {
    let raw_fd = master.as_raw_fd();
    // PTY를 논블로킹 모드로 설정
    if unsafe { fcntl(raw_fd, nix::libc::F_SETFL, nix::libc::O_NONBLOCK) } != 0 {
        let errno = std::io::Error::last_os_error();
        bail!("fcntl error: {errno:?}");
    }
    let mut file = unsafe { File::from_raw_fd(raw_fd) };
    let mut stdout = std::io::stdout();
    let stdin = std::io::stdin();
    let stdin_fd = stdin.as_raw_fd();
    // 터미널 설정 저장 및 에코 비활성화
    let old_termios = termios::Termios::from_fd(stdin_fd)?;
    turn_off_echo(stdin_fd).ok();
    // 프로세스 종료 시 에코 복구 후크 등록
    shutdown_hooks::add_shutdown_hook(turn_on_echo_shutdown_hook);
    // 표준 입력 읽기를 위한 채널
    let (tx, rx) = channel::<Vec<u8>>();
    // 표준 입력을 별도 스레드에서 읽기
    std::thread::spawn(move || loop {
        let mut line = String::default();
        match stdin.read_line(&mut line) {
            Ok(0) => {
                // EOF 수신
                kill_child(child);
                break;
            }
            Ok(_) => {
                // 사용자 입력 전달
                if let Err(e) = tx.send(line.as_bytes().to_vec()) {
                    log::error!("Channel error: {e:?}");
                    kill_child(child);
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                log::info!("Failed to read stdin: {e:?}");
                kill_child(child);
                break;
            }
        };
    });
    // 자식 프로세스 입출력 중계 루프
    loop {
        let mut buf = [0; 1024];
        match file.read(&mut buf) {
            Ok(0) => {
                log::info!("read from child: EOF");
                break;
            }
            Ok(n) => {
                // 자식 프로세스 출력을 표준 출력으로 표시
                let buf = String::from_utf8_lossy(&buf[..n]).to_string();
                print!("{}", buf);
                if let Err(e) = stdout.flush() {
                    log::error!("flush failed: {e:?}");
                    kill_child(child);
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // 데이터 없을 때 대기
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                // 자식 프로세스 종료
                log::info!("Read child error: {:?}", e);
                break;
            }
        }
        // 사용자 입력을 자식 프로세스로 전달
        match rx.try_recv() {
            Ok(v) => {
                if let Err(e) = file.write_all(&v) {
                    log::error!("write error: {e:?}");
                    kill_child(child);
                    break;
                }
            }
            Err(e) => match e {
                std::sync::mpsc::TryRecvError::Empty => {}
                std::sync::mpsc::TryRecvError::Disconnected => {
                    log::error!("receive error: {e:?}");
                    kill_child(child);
                    break;
                }
            },
        }
    }

    // 자식 프로세스 종료 대기
    let status = waitpid(child, None);
    log::info!("waitpid status: {:?}", status);
    let mut code = EXIT_CODE;
    match status {
        Ok(s) => match s {
            nix::sys::wait::WaitStatus::Exited(_pid, status) => {
                code = status;
            }
            _ => {}
        },
        Err(_) => {}
    }
    // 터미널 설정 복구
    termios::tcsetattr(stdin_fd, termios::TCSANOW, &old_termios).ok();
    std::process::exit(code);
}

/// GUI 모드 부모 프로세스 처리
/// 자식 프로세스의 출력을 모니터링하여 암호 입력 프롬프트 감지
/// 사용자 입력을 받아 자식 프로세스로 전달
fn ui_parent(
    child: Pid,
    master: OwnedFd,
    tx_to_ui: Sender<Message>,
    rx_from_ui: Receiver<Message>,
    is_su: bool,
    initial_password: Option<String>,
) -> ResultType<()> {
    let mut initial_password = initial_password;
    let raw_fd = master.as_raw_fd();
    // PTY를 논블로킹 모드로 설정
    if unsafe { fcntl(raw_fd, nix::libc::F_SETFL, nix::libc::O_NONBLOCK) } != 0 {
        let errno = std::io::Error::last_os_error();
        tx_to_ui.send(Message::ErrorDialog(format!("fcntl error: {errno:?}")))?;
        bail!("fcntl error: {errno:?}");
    }
    let mut file = unsafe { File::from_raw_fd(raw_fd) };

    // 암호 프롬프트 상태 추적
    let mut first = initial_password.is_none();
    let mut su_password_sent = false;
    // 명령 실행 결과 저장 (에러 표시용)
    let mut saved_output = String::default();
    loop {
        let mut buf = [0; 1024];
        match file.read(&mut buf) {
            Ok(0) => {
                log::info!("read from child: EOF");
                break;
            }
            Ok(n) => {
                saved_output = String::default();
                let buf = String::from_utf8_lossy(&buf[..n]).trim().to_string();
                let last_line = buf.lines().last().unwrap_or(&buf).trim().to_string();
                log::info!("read from child: {}", buf);

                if last_line.starts_with("sudo:") || last_line.starts_with("su:") {
                    if let Err(e) = tx_to_ui.send(Message::ErrorDialog(last_line)) {
                        log::error!("Channel error: {e:?}");
                        kill_child(child);
                    }
                    break;
                } else if last_line.ends_with(":") {
                    match get_echo_turn_off(raw_fd) {
                        Ok(true) => {
                            log::debug!("get_echo_turn_off ok");
                            if let Some(password) = initial_password.clone() {
                                let v = format!("{}\n", password);
                                if let Err(e) = file.write_all(v.as_bytes()) {
                                    let e = format!("Failed to send password: {e:?}");
                                    if let Err(e) = tx_to_ui.send(Message::ErrorDialog(e)) {
                                        log::error!("Channel error: {e:?}");
                                    }
                                    kill_child(child);
                                    break;
                                }
                                if is_su && !su_password_sent {
                                    su_password_sent = true;
                                    continue;
                                }
                                initial_password = None;
                                continue;
                            }
                            // In fact, su mode can only input password once
                            let err_msg = if first { "" } else { "Sorry, try again." };
                            first = false;
                            if let Err(e) =
                                tx_to_ui.send(Message::PasswordPrompt((err_msg.to_string(), false)))
                            {
                                log::error!("Channel error: {e:?}");
                                kill_child(child);
                                break;
                            }
                            match rx_from_ui.recv() {
                                Ok(Message::Password((_, password))) => {
                                    let v = format!("{}\n", password);
                                    if let Err(e) = file.write_all(v.as_bytes()) {
                                        let e = format!("Failed to send password: {e:?}");
                                        if let Err(e) = tx_to_ui.send(Message::ErrorDialog(e)) {
                                            log::error!("Channel error: {e:?}");
                                        }
                                        kill_child(child);
                                        break;
                                    }
                                }
                                Ok(Message::Cancel) => {
                                    log::info!("User canceled");
                                    kill_child(child);
                                    break;
                                }
                                _ => {
                                    log::error!("Unexpected message");
                                    break;
                                }
                            }
                        }
                        Ok(false) => log::warn!("get_echo_turn_off timeout"),
                        Err(e) => log::error!("get_echo_turn_off error: {:?}", e),
                    }
                } else {
                    saved_output = buf.clone();
                    if !last_line.is_empty() && initial_password.is_some() {
                        log::error!("received not empty line: {last_line}, clear initial password");
                        initial_password = None;
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                // Child process is dead
                log::debug!("Read error: {:?}", e);
                break;
            }
        }
    }

    // Wait for child process
    let status = waitpid(child, None);
    log::info!("waitpid status: {:?}", status);
    let mut code = EXIT_CODE;
    match status {
        Ok(s) => match s {
            nix::sys::wait::WaitStatus::Exited(_pid, status) => {
                code = status;
            }
            _ => {}
        },
        Err(_) => {}
    }

    if code != 0 && !saved_output.is_empty() {
        if let Err(e) = tx_to_ui.send(Message::ErrorDialog(saved_output.clone())) {
            log::error!("Channel error: {e:?}");
            std::process::exit(code);
        }
        return Ok(());
    }
    if let Err(e) = tx_to_ui.send(Message::Exit(code)) {
        log::error!("Channel error: {e:?}");
        std::process::exit(code);
    }
    Ok(())
}

/// 자식 프로세스: sudo 또는 su 명령 실행
/// 쉘을 통해 주어진 명령을 실행하며, 필요한 경우 사용자 변경을 수행
/// 참고: https://doc.rust-lang.org/std/env/consts/constant.OS.html
fn child(su_user: Option<String>, args: Vec<String>) -> ResultType<()> {
    // OS 확인
    let os = std::env::consts::OS;
    // BSD 계열 OS 판별 (LC_ALL 처리 방식이 다름)
    let bsd = os == "freebsd" || os == "dragonfly" || os == "netbsd" || os == "openbsd";
    let mut params = vec!["sudo".to_string()];
    // su 사용 모드일 때는 stdin에서 암호 받기 (-S 옵션)
    if su_user.is_some() {
        params.push("-S".to_string());
    }
    params.push(CMD_SH.to_string());
    params.push("-c".to_string());

    // 명령 인자들을 쉘 명령어로 변환
    let command = args
        .iter()
        .map(|s| {
            if su_user.is_some() {
                s.to_string()
            } else {
                // 특수문자 이스케이프 처리
                quote_shell_arg(s, true)
            }
        })
        .collect::<Vec<String>>()
        .join(" ");
    // BSD에서는 LC_ALL 설정 필요
    let mut command = if bsd {
        let lc = match std::env::var("LC_ALL") {
            Ok(lc_all) => {
                // 주입 공격 방지: LC_ALL에 따옴표가 있으면 거부
                if lc_all.contains('\'') {
                    eprintln!(
                        "sudo: Detected attempt to inject privileged command via LC_ALL env({lc_all}). Exiting!\n",
                    );
                    std::process::exit(EXIT_CODE);
                }
                format!("LC_ALL='{lc_all}' ")
            }
            Err(_) => {
                format!("unset LC_ALL;")
            }
        };
        format!("{}exec {}", lc, command)
    } else {
        command.to_string()
    };
    if su_user.is_some() {
        command = format!("'{}'", quote_shell_arg(&command, false));
    }
    params.push(command);
    std::env::set_var("LC_ALL", "C");

    // su로 사용자 변경이 필요한 경우 su 명령 구성
    if let Some(user) = &su_user {
        let su_subcommand = params
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<String>>()
            .join(" ");
        params = vec![
            "su".to_string(),
            "-".to_string(),
            user.to_string(),
            "-c".to_string(),
            su_subcommand,
        ];
    }

    // 세션 리더 생성 (실패해도 계속 진행)
    let _ = setsid();
    // C 문자열로 변환
    let mut cparams = vec![];
    for param in &params {
        cparams.push(CString::new(param.as_str())?);
    }
    let su_or_sudo = if su_user.is_some() { "su" } else { "sudo" };
    // sudo/su 명령으로 프로세스 이미지 교체
    let res = execvp(CString::new(su_or_sudo)?.as_c_str(), &cparams);
    eprintln!("sudo: execvp error: {:?}", res);
    std::process::exit(EXIT_CODE);
}

/// 터미널 에코가 비활성화될 때까지 대기 (최대 100ms)
/// 암호 입력 프롬프트 감지를 위해 사용
fn get_echo_turn_off(fd: RawFd) -> Result<bool, Error> {
    let tios = termios::Termios::from_fd(fd)?;
    // 최대 10회 반복, 10ms 간격으로 에코 비활성 상태 확인
    for _ in 0..10 {
        if tios.c_lflag & termios::ECHO == 0 {
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Ok(false)
}

/// 터미널 에코 비활성화 (암호 입력을 위해)
fn turn_off_echo(fd: RawFd) -> Result<(), Error> {
    use termios::*;
    let mut termios = Termios::from_fd(fd)?;
    // ECHO 플래그만 비활성화 (다른 플래그는 유지)
    // termios.c_lflag &= !(ECHO | ECHONL | ICANON | IEXTEN);
    termios.c_lflag &= !ECHO;
    tcsetattr(fd, TCSANOW, &termios)?;
    Ok(())
}

// 프로세스 종료 시 터미널 에코 복구
// 비정상 종료 시에도 터미널 설정이 원래대로 복구되도록 함
pub extern "C" fn turn_on_echo_shutdown_hook() {
    let fd = std::io::stdin().as_raw_fd();
    if let Ok(mut termios) = termios::Termios::from_fd(fd) {
        // ECHO 플래그 활성화
        termios.c_lflag |= termios::ECHO;
        termios::tcsetattr(fd, termios::TCSANOW, &termios).ok();
    }
}

/// 자식 프로세스 종료
/// 먼저 SIGINT 신호를 보낸 후, 최대 1초 대기
/// 응답 없으면 SIGKILL 신호로 강제 종료
fn kill_child(child: Pid) {
    // 부드러운 종료 신호 전송
    unsafe { kill(child.as_raw(), Signal::SIGINT as _) };
    let mut res = 0;

    // 최대 1초 동안 프로세스 종료 대기 (100ms * 10회)
    for _ in 0..10 {
        match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
            Ok(_) => {
                res = 1;
                break;
            }
            Err(_) => (),
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // 여전히 살아있으면 강제 종료
    if res == 0 {
        log::info!("Force killing child process");
        unsafe { kill(child.as_raw(), Signal::SIGKILL as _) };
    }
}

/// 암호 입력 대화상자 표시 및 사용자 입력 받기
/// 매개변수:
/// - username: 표시할 사용자명
/// - last_password: 마지막 입력 암호 (미리 채우기용)
/// - err: 에러 메시지 (이전 실패 시)
/// - show_edit: 사용자명 수정 가능 여부
fn password_prompt(
    username: &str,
    last_password: &str,
    err: &str,
    show_edit: bool,
) -> Option<(String, String)> {
    let dialog = gtk::Dialog::builder()
        .title(crate::get_app_name())
        .modal(true)
        .build();
    // 기본 응답 설정
    // 참고: https://docs.gtk.org/gtk4/method.Dialog.set_default_response.html
    dialog.set_default_response(gtk::ResponseType::Ok);
    let content_area = dialog.content_area();

    let label = gtk::Label::builder()
        .label(translate("Authentication Required".to_string()))
        .margin_top(10)
        .build();
    content_area.add(&label);

    let image = gtk::Image::from_icon_name(Some("avatar-default-symbolic"), gtk::IconSize::Dialog);
    image.set_margin_top(10);
    content_area.add(&image);

    let user_label = gtk::Label::new(Some(username));
    let edit_button = gtk::Button::new();
    edit_button.set_relief(gtk::ReliefStyle::None);
    let edit_icon =
        gtk::Image::from_icon_name(Some("document-edit-symbolic"), gtk::IconSize::Button.into());
    edit_button.set_image(Some(&edit_icon));
    edit_button.set_can_focus(false);
    let user_entry = gtk::Entry::new();
    user_entry.set_alignment(0.5);
    user_entry.set_width_request(100);
    let user_box = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    user_box.add(&user_label);
    user_box.add(&edit_button);
    user_box.add(&user_entry);
    user_box.set_halign(gtk::Align::Center);
    user_box.set_valign(gtk::Align::Center);
    user_box.set_vexpand(true);
    content_area.add(&user_box);

    edit_button.connect_clicked(
        glib::clone!(@weak user_label, @weak edit_button, @weak user_entry=>  move |_| {
            let username = user_label.text().to_string();
            user_entry.set_text(&username);
            user_label.hide();
            edit_button.hide();
            user_entry.show();
            user_entry.grab_focus();
        }),
    );

    // 암호 입력 필드 생성 (텍스트 숨김)
    let password_input = gtk::Entry::builder()
        .visibility(false)
        .input_purpose(gtk::InputPurpose::Password)
        .placeholder_text(translate("Password".to_string()))
        .margin_top(20)
        .margin_start(30)
        .margin_end(30)
        .activates_default(true)
        .text(last_password)
        .build();
    password_input.set_alignment(0.5);
    // Enter 키 입력 시 대화상자 확인
    // 참고: https://docs.gtk.org/gtk3/signal.Entry.activate.html
    password_input.connect_activate(glib::clone!(@weak dialog => move |_| {
        dialog.response(gtk::ResponseType::Ok);
    }));
    content_area.add(&password_input);

    user_entry.connect_focus_out_event(
        glib::clone!(@weak user_label, @weak edit_button, @weak user_entry, @weak password_input => @default-return glib::Propagation::Proceed,  move |_, _| {
            let username = user_entry.text().to_string();
            user_label.set_text(&username);
            user_entry.hide();
            user_label.show();
            edit_button.show();
            glib::Propagation::Proceed
        }),
    );
    user_entry.connect_activate(
        glib::clone!(@weak user_label, @weak edit_button, @weak user_entry, @weak password_input => move |_| {
            let username = user_entry.text().to_string();
            user_label.set_text(&username);
            user_entry.hide();
            user_label.show();
            edit_button.show();
            password_input.grab_focus();
        }),
    );

    if !err.is_empty() {
        let err_label = gtk::Label::new(None);
        err_label.set_markup(&format!(
            "<span font='10' foreground='orange'>{}</span>",
            err
        ));
        err_label.set_selectable(true);
        content_area.add(&err_label);
    }

    let cancel_button = gtk::Button::builder()
        .label(translate("Cancel".to_string()))
        .hexpand(true)
        .build();
    cancel_button.connect_clicked(glib::clone!(@weak dialog => move |_| {
        dialog.response(gtk::ResponseType::Cancel);
    }));
    let authenticate_button = gtk::Button::builder()
        .label(translate("Authenticate".to_string()))
        .hexpand(true)
        .build();
    authenticate_button.connect_clicked(glib::clone!(@weak dialog => move |_| {
        dialog.response(gtk::ResponseType::Ok);
    }));
    let button_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .hexpand(true)
        .homogeneous(true)
        .spacing(10)
        .margin_top(10)
        .build();
    button_box.add(&cancel_button);
    button_box.add(&authenticate_button);
    content_area.add(&button_box);

    content_area.set_spacing(10);
    content_area.set_border_width(10);

    dialog.set_width_request(400);
    dialog.show_all();
    dialog.set_position(gtk::WindowPosition::Center);
    dialog.set_keep_above(true);
    password_input.grab_focus();
    user_entry.hide();
    if !show_edit {
        edit_button.hide();
    }
    dialog.check_resize();
    let response = dialog.run();
    dialog.hide();

    if response == gtk::ResponseType::Ok {
        let username = if user_entry.get_visible() {
            user_entry.text().to_string()
        } else {
            user_label.text().to_string()
        };
        Some((username, password_input.text().to_string()))
    } else {
        None
    }
}

/// 에러 메시지 표시 및 프로세스 종료
fn error_dialog_and_exit(err_msg: &str, exit_code: i32) {
    log::error!("Error dialog: {err_msg}, exit code: {exit_code}");
    let dialog = gtk::MessageDialog::builder()
        .message_type(gtk::MessageType::Error)
        .title(crate::get_app_name())
        .text("Error")
        .secondary_text(err_msg)
        .modal(true)
        .buttons(gtk::ButtonsType::Ok)
        .build();
    dialog.set_position(gtk::WindowPosition::Center);
    dialog.set_keep_above(true);
    dialog.run();
    dialog.close();
    std::process::exit(exit_code);
}

/// 쉘 인자를 안전하게 이스케이프
/// 특수문자가 있으면 따옴표로 감싸고, 기존 따옴표는 이스케이프 처리
/// add_splash_if_match: true이면 특수문자 있을 때 따옴표로 감쌈
fn quote_shell_arg(arg: &str, add_splash_if_match: bool) -> String {
    let mut rv = arg.to_string();
    // 쉘 특수문자 정규표현식
    let re = hbb_common::regex::Regex::new("(\\s|[][!\"#$&'()*,;<=>?\\^`{}|~])");
    let Ok(re) = re else {
        return rv;
    };
    if re.is_match(arg) {
        // 기존 따옴표 이스케이프
        rv = rv.replace("'", "'\\''");
        if add_splash_if_match {
            rv = format!("'{}'", rv);
        }
    }
    rv
}
