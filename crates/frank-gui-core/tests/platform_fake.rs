//! Confirms `Platform` is object-safe/implementable the way M-4's real
//! platform shell (backed by `tray-icon`/`muda`/`auto-launch`) will need,
//! without pulling any of those main-thread-affine crates into this
//! coverage/mutation-gated crate.

use std::cell::RefCell;

use frank_gui_core::Platform;

#[derive(Default)]
struct FakePlatform {
    tray_installs: RefCell<Vec<(Vec<String>, Option<String>)>>,
    tray_status_updates: RefCell<Vec<Option<String>>>,
    autostart_calls: RefCell<Vec<bool>>,
    autostart_should_fail: bool,
}

impl Platform for FakePlatform {
    fn install_tray(&self, levels: &[String], active: Option<&str>) -> Result<(), String> {
        self.tray_installs
            .borrow_mut()
            .push((levels.to_vec(), active.map(str::to_string)));
        Ok(())
    }

    fn update_tray_status(&self, active: Option<&str>) {
        self.tray_status_updates
            .borrow_mut()
            .push(active.map(str::to_string));
    }

    fn set_autostart(&self, enabled: bool) -> Result<(), String> {
        self.autostart_calls.borrow_mut().push(enabled);
        if self.autostart_should_fail {
            Err("denied".to_string())
        } else {
            Ok(())
        }
    }
}

#[test]
fn fake_platform_records_tray_install_and_status_updates() {
    let platform = FakePlatform::default();
    platform
        .install_tray(&["full".to_string(), "lite".to_string()], Some("full"))
        .unwrap();
    platform.update_tray_status(None);
    platform.update_tray_status(Some("lite"));

    assert_eq!(
        *platform.tray_installs.borrow(),
        vec![(
            vec!["full".to_string(), "lite".to_string()],
            Some("full".to_string())
        )]
    );
    assert_eq!(
        *platform.tray_status_updates.borrow(),
        vec![None, Some("lite".to_string())]
    );
}

#[test]
fn fake_platform_surfaces_autostart_failures() {
    let platform = FakePlatform {
        autostart_should_fail: true,
        ..Default::default()
    };
    let result = platform.set_autostart(true);
    assert_eq!(result, Err("denied".to_string()));
    assert_eq!(*platform.autostart_calls.borrow(), vec![true]);
}
