use super::*;

#[test]
fn menu_ids_map_to_what_they_say() {
    assert_eq!(pick_for_menu("setup"), Some(Pick::Setup));
    assert_eq!(pick_for_menu("quit"), Some(Pick::Quit));
    assert_eq!(pick_for_menu("Hold Ctrl + Space to talk"), None);
}
