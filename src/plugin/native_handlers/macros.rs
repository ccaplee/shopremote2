/// 메서드가 주어진 접두사로 시작하지 않으면 None을 반환하는 매크로
#[macro_export]
macro_rules! return_if_not_method {
    ($call: ident, $prefix: ident) => {
        if $call.starts_with($prefix) {
            return None;
        }
    };
}

/// 메서드가 주어진 값과 다르면 블록을 실행하는 매크로
#[macro_export]
macro_rules! call_if_method {
    ($call: ident ,$method: literal, $block: block) => {
        if ($call != $method) {
            $block
        }
    };
}

/// 메서드 접두사를 정의하는 매크로
#[macro_export]
macro_rules! define_method_prefix {
    ($prefix: literal) => {
        #[inline]
        fn method_prefix(&self) -> &'static str {
            $prefix
        }
    };
}
