use hbb_common::{fs, log, message_proto::*};

use super::{Data, Interface};

/// 파일 관리 기능을 제공하는 트레이트
/// 클라이언트에서 로컬 및 원격 파일 시스템 조작을 수행하는 모든 메서드를 정의
pub trait FileManager: Interface {
    /// 홈 디렉토리 경로 반환
    /// 안드로이드, iOS, CLI, Flutter를 제외한 플랫폼에서만 사용 가능
    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        feature = "cli",
        feature = "flutter"
    )))]
    fn get_home_dir(&self) -> String {
        fs::get_home_as_string()
    }

    /// 다음 파일 전송 작업의 ID를 조회
    /// 안드로이드, iOS, CLI, Flutter를 제외한 플랫폼에서만 사용 가능
    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        feature = "cli",
        feature = "flutter"
    )))]
    fn get_next_job_id(&self) -> i32 {
        fs::get_next_job_id()
    }

    /// 다음 파일 전송 작업 ID 업데이트
    /// 안드로이드, iOS, CLI, Flutter를 제외한 플랫폼에서만 사용 가능
    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        feature = "cli",
        feature = "flutter"
    )))]
    fn update_next_job_id(&self, id: i32) {
        fs::update_next_job_id(id);
    }

    /// 로컬 디렉토리 내용 읽기
    /// path: 읽을 디렉토리 경로
    /// include_hidden: 숨김 파일 포함 여부
    /// 반환값: 디렉토리 정보를 포함한 sciter Value 객체
    /// 안드로이드, iOS, CLI, Flutter를 제외한 플랫폼에서만 사용 가능
    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        feature = "cli",
        feature = "flutter"
    )))]
    fn read_dir(&self, path: String, include_hidden: bool) -> sciter::Value {
        match fs::read_dir(&fs::get_path(&path), include_hidden) {
            Err(_) => sciter::Value::null(),
            Ok(fd) => {
                use crate::ui::remote::make_fd;
                let mut m = make_fd(0, &fd.entries.to_vec(), false);
                m.set_item("path", path);
                m
            }
        }
    }

    /// 진행 중인 파일 전송 작업 취소
    /// id: 취소할 작업의 ID
    fn cancel_job(&self, id: i32) {
        self.send(Data::CancelJob(id));
    }

    /// 원격 서버에서 빈 디렉토리 목록 읽기 요청
    /// path: 스캔할 디렉토리 경로
    /// include_hidden: 숨김 파일 포함 여부
    fn read_empty_dirs(&self, path: String, include_hidden: bool) {
        let mut msg_out = Message::new();
        let mut file_action = FileAction::new();
        file_action.set_read_empty_dirs(ReadEmptyDirs {
            path,
            include_hidden,
            ..Default::default()
        });
        msg_out.set_file_action(file_action);
        self.send(Data::Message(msg_out));
    }

    /// 원격 서버에서 디렉토리 내용 읽기 요청
    /// path: 읽을 디렉토리 경로
    /// include_hidden: 숨김 파일 포함 여부
    fn read_remote_dir(&self, path: String, include_hidden: bool) {
        let mut msg_out = Message::new();
        let mut file_action = FileAction::new();
        file_action.set_read_dir(ReadDir {
            path,
            include_hidden,
            ..Default::default()
        });
        msg_out.set_file_action(file_action);
        self.send(Data::Message(msg_out));
    }

    /// 파일 삭제 요청 송신
    /// id: 작업 ID
    /// path: 삭제할 파일의 경로
    /// file_num: 삭제할 파일의 개수
    /// is_remote: 원격 파일 여부
    fn remove_file(&self, id: i32, path: String, file_num: i32, is_remote: bool) {
        self.send(Data::RemoveFile((id, path, file_num, is_remote)));
    }

    /// 디렉토리 및 하위 모든 항목 삭제 요청
    /// id: 작업 ID
    /// path: 삭제할 디렉토리 경로
    /// is_remote: 원격 디렉토리 여부
    /// include_hidden: 숨김 파일 포함 여부
    fn remove_dir_all(&self, id: i32, path: String, is_remote: bool, include_hidden: bool) {
        self.send(Data::RemoveDirAll((id, path, is_remote, include_hidden)));
    }

    /// 파일 삭제 확인 메시지 송신
    /// id: 작업 ID
    /// file_num: 삭제할 파일 개수
    /// 안드로이드, iOS, CLI, Flutter를 제외한 플랫폼에서만 사용 가능
    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        feature = "cli",
        feature = "flutter"
    )))]
    fn confirm_delete_files(&self, id: i32, file_num: i32) {
        self.send(Data::ConfirmDeleteFiles((id, file_num)));
    }

    /// 파일 삭제 시 확인 메시지 표시 안 함 설정
    /// id: 작업 ID
    /// 안드로이드, iOS, CLI, Flutter를 제외한 플랫폼에서만 사용 가능
    #[cfg(not(any(
        target_os = "android",
        target_os = "ios",
        feature = "cli",
        feature = "flutter"
    )))]
    fn set_no_confirm(&self, id: i32) {
        self.send(Data::SetNoConfirm(id));
    }

    /// 디렉토리 삭제 요청
    /// 원격인 경우: 원격 서버에 요청 전송
    /// 로컬인 경우: 직접 로컬 파일시스템에서 빈 디렉토리 제거
    /// id: 작업 ID
    /// path: 삭제할 디렉토리 경로
    /// is_remote: 원격 디렉토리 여부
    fn remove_dir(&self, id: i32, path: String, is_remote: bool) {
        if is_remote {
            self.send(Data::RemoveDir((id, path)));
        } else {
            fs::remove_all_empty_dir(&fs::get_path(&path)).ok();
        }
    }

    /// 새 디렉토리 생성 요청
    /// id: 작업 ID
    /// path: 생성할 디렉토리의 경로
    /// is_remote: 원격 위치에 생성할지 여부
    fn create_dir(&self, id: i32, path: String, is_remote: bool) {
        self.send(Data::CreateDir((id, path, is_remote)));
    }

    /// 파일 전송 요청
    /// id: 작업 ID
    /// type: 작업 타입 (복사, 이동 등)
    /// path: 소스 경로
    /// to: 대상 경로
    /// file_num: 전송할 파일 개수
    /// include_hidden: 숨김 파일 포함 여부
    /// is_remote: 원격 전송 여부
    fn send_files(
        &self,
        id: i32,
        r#type: i32,
        path: String,
        to: String,
        file_num: i32,
        include_hidden: bool,
        is_remote: bool,
    ) {
        self.send(Data::SendFiles((
            id,
            r#type.into(),
            path,
            to,
            file_num,
            include_hidden,
            is_remote,
        )));
    }

    /// 파일 전송 작업을 작업 큐에 추가
    /// id: 작업 ID
    /// type: 작업 타입
    /// path: 소스 경로
    /// to: 대상 경로
    /// file_num: 작업할 파일 개수
    /// include_hidden: 숨김 파일 포함 여부
    /// is_remote: 원격 작업 여부
    fn add_job(
        &self,
        id: i32,
        r#type: i32,
        path: String,
        to: String,
        file_num: i32,
        include_hidden: bool,
        is_remote: bool,
    ) {
        self.send(Data::AddJob((
            id,
            r#type.into(),
            path,
            to,
            file_num,
            include_hidden,
            is_remote,
        )));
    }

    /// 일시 중지된 파일 전송 작업 재개
    /// id: 작업 ID
    /// is_remote: 원격 작업 여부
    fn resume_job(&self, id: i32, is_remote: bool) {
        self.send(Data::ResumeJob((id, is_remote)));
    }

    /// 파일 덮어쓰기 확인 설정
    /// id: 작업 ID
    /// file_num: 파일 번호
    /// need_override: 덮어쓰기 필요 여부
    /// remember: 선택사항 기억 여부
    /// is_upload: 업로드 작업 여부
    fn set_confirm_override_file(
        &self,
        id: i32,
        file_num: i32,
        need_override: bool,
        remember: bool,
        is_upload: bool,
    ) {
        log::info!(
            "파일 전송 확인, 작업: {}, 덮어쓰기 필요: {}",
            id,
            need_override
        );
        self.send(Data::SetConfirmOverrideFile((
            id,
            file_num,
            need_override,
            remember,
            is_upload,
        )));
    }

    /// 파일 또는 디렉토리 이름 변경 요청
    /// act_id: 작업 ID
    /// path: 변경할 파일의 경로
    /// new_name: 새로운 이름
    /// is_remote: 원격 파일 여부
    fn rename_file(&self, act_id: i32, path: String, new_name: String, is_remote: bool) {
        self.send(Data::RenameFile((act_id, path, new_name, is_remote)));
    }
}
