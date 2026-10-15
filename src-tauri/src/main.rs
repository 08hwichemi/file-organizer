// 배포(release) 빌드에서 Windows 콘솔 창이 함께 뜨지 않게 한다. 지우지 말 것.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    file_organizer_lib::run()
}
