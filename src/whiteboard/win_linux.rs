use hbb_common::{bail, ResultType};
use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Point, Rect, Transform};
use ttf_parser::Face;

/// ttf-parser와 tiny-skia를 연결하는 헬퍼 구조체
/// 글꼴 아웃라인을 벡터 경로로 변환합니다.
struct PathBuilderWrapper<'a> {
    /// 경로 구성 객체
    path_builder: &'a mut PathBuilder,
    /// 경로 변환 매트릭스 (확대, 회전 등)
    transform: Transform,
}

impl ttf_parser::OutlineBuilder for PathBuilderWrapper<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let mut pt = Point::from_xy(x, y);
        self.transform.map_point(&mut pt);
        self.path_builder.move_to(pt.x, pt.y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let mut pt = Point::from_xy(x, y);
        self.transform.map_point(&mut pt);
        self.path_builder.line_to(pt.x, pt.y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let mut pt1 = Point::from_xy(x1, y1);
        self.transform.map_point(&mut pt1);
        let mut pt = Point::from_xy(x, y);
        self.transform.map_point(&mut pt);
        self.path_builder.quad_to(pt1.x, pt1.y, pt.x, pt.y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let mut pt1 = Point::from_xy(x1, y1);
        self.transform.map_point(&mut pt1);
        let mut pt2 = Point::from_xy(x2, y2);
        self.transform.map_point(&mut pt2);
        let mut pt = Point::from_xy(x, y);
        self.transform.map_point(&mut pt);
        self.path_builder
            .cubic_to(pt1.x, pt1.y, pt2.x, pt2.y, pt.x, pt.y);
    }

    fn close(&mut self) {
        self.path_builder.close();
    }
}

// Draws a string of text with the white background rectangle onto the pixmap.
pub(super) fn draw_text(
    pixmap: &mut PixmapMut,
    face: &Face,
    text: &str,
    x: f32,
    y: f32,
    paint: &Paint,
    font_size: f32,
) {
    let units_per_em = face.units_per_em() as f32;
    let scale = font_size / units_per_em;

    // === 1. 배경 사각형의 크기 계산 ===
    let mut total_width = 0.0;
    // 텍스트의 전체 너비 계산
    for ch in text.chars() {
        let glyph_id = face.glyph_index(ch).unwrap_or_default();
        if let Some(h_advance) = face.glyph_hor_advance(glyph_id) {
            total_width += h_advance as f32 * scale;
        }
    }

    // 폰트 메트릭을 사용하여 일관된 배경 높이 설정
    let font_height = (face.ascender() - face.descender()) as f32 * scale;
    let ascent = face.ascender() as f32 * scale;
    // 텍스트 주위에 패딩 추가
    let padding = 3.0;

    let mut bg_filled = false;
    // === 2. 흰색 배경 사각형 그리기 (모서리가 둥근 직사각형) ===
    if let Some(bg_rect) = Rect::from_xywh(
        x - padding,
        y - ascent - padding,
        total_width + 2.0 * padding,
        font_height + 2.0 * padding,
    ) {
        // 모서리 반지름
        let radius = 5.0;
        let path = {
            let mut pb = PathBuilder::new();
            let r_x = bg_rect.x();
            let r_y = bg_rect.y();
            let r_w = bg_rect.width();
            let r_h = bg_rect.height();
            pb.move_to(r_x + radius, r_y);
            pb.line_to(r_x + r_w - radius, r_y);
            pb.quad_to(r_x + r_w, r_y, r_x + r_w, r_y + radius);
            pb.line_to(r_x + r_w, r_y + r_h - radius);
            pb.quad_to(r_x + r_w, r_y + r_h, r_x + r_w - radius, r_y + r_h);
            pb.line_to(r_x + radius, r_y + r_h);
            pb.quad_to(r_x, r_y + r_h, r_x, r_y + r_h - radius);
            pb.line_to(r_x, r_y + radius);
            pb.quad_to(r_x, r_y, r_x + radius, r_y);
            pb.close();
            pb.finish()
        };

        if let Some(path) = path {
            let mut bg_paint = Paint::default();
            bg_paint.set_color_rgba8(255, 255, 255, 255);
            bg_paint.anti_alias = true;
            pixmap.fill_path(
                &path,
                &bg_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
            bg_filled = true;
        }
    }

    // === 3. 텍스트 그리기 ===
    // 변환 매트릭스: X, Y로 이동하고, 스케일을 적용하며, Y축을 반전
    let transform = Transform::from_translate(x, y).pre_scale(scale, -scale);
    let mut path_builder = PathBuilder::new();
    let mut current_x = 0.0;

    // 각 글자의 경로 생성
    for ch in text.chars() {
        let glyph_id = face.glyph_index(ch).unwrap_or_default();

        let mut builder = PathBuilderWrapper {
            path_builder: &mut path_builder,
            // 현재 글자의 위치에 맞게 변환 적용
            transform: transform.post_translate(current_x, 0.0),
        };

        // 글꼴에서 글리프 아웃라인 추출
        face.outline_glyph(glyph_id, &mut builder);

        // 다음 글자를 위해 X 위치 이동
        if let Some(h_advance) = face.glyph_hor_advance(glyph_id) {
            current_x += h_advance as f32 * scale;
        }
    }

    // 텍스트 경로 그리기
    if let Some(path) = path_builder.finish() {
        if bg_filled {
            // 배경이 있으면 검은색으로 텍스트 그리기
            let mut text_paint = Paint::default();
            text_paint.set_color_rgba8(0, 0, 0, 255);
            text_paint.anti_alias = true;
            pixmap.fill_path(
                &path,
                &text_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        } else {
            // 배경이 없으면 지정된 색상으로 그리기
            pixmap.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
        }
    }
}

/// 시스템 글꼴에서 커서 텍스트 렌더링용 글꼴 면(face)을 생성합니다.
/// Monospace 또는 SansSerif 글꼴을 우선적으로 사용합니다.
pub(super) fn create_font_face() -> ResultType<Face<'static>> {
    // 시스템의 모든 글꼴 로드
    let mut font_db = fontdb::Database::new();
    font_db.load_system_fonts();

    // Monospace 또는 SansSerif 글꼴 검색
    let query = fontdb::Query {
        families: &[fontdb::Family::Monospace, fontdb::Family::SansSerif],
        ..fontdb::Query::default()
    };

    let Some(font_id) = font_db.query(&query) else {
        bail!("Monospace 또는 SansSerif 글꼴을 찾을 수 없음");
    };

    let Some((font_source, face_index)) = font_db.face_source(font_id) else {
        bail!("글꼴의 face를 찾을 수 없음");
    };

    // 글꼴 데이터를 정적 수명으로 로드
    // ttf-parser의 수명 요구사항을 만족하기 위해 Box::leak을 사용합니다.
    // 글꼴 데이터는 응용프로그램의 전체 생명주기 동안 필요하므로 이는 허용됩니다.
    let font_data: &'static [u8] = Box::leak(match font_source {
        fontdb::Source::File(path) => std::fs::read(path)?.into_boxed_slice(),
        fontdb::Source::Binary(data) => data.as_ref().as_ref().to_vec().into_boxed_slice(),
        fontdb::Source::SharedFile(path, _) => std::fs::read(path)?.into_boxed_slice(),
    });

    // 글꼴 파싱
    let face = Face::parse(font_data, face_index)?;
    Ok(face)
}
