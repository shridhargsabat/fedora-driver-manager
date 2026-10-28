use crate::backend::hardware::{HardwareScanner, ScanReport};
use crate::backend::system::{RootAction, SystemBackend};
use crate::models::{DeviceCategory, DnfTransaction, DriverPackage, DriverStatus, FirmwareItem, HardwareDevice, PackageUpdate};
use adw::prelude::*;
use adw::{ActionRow, ExpanderRow, HeaderBar, NavigationPage, NavigationSplitView, PreferencesGroup, PreferencesPage, Toast, ToastOverlay, WindowTitle};
use gtk4::prelude::{IsA, WidgetExt};
use gio::prelude::ActionMapExt;
use gtk4::{Align, Box, Button, Image, Label, Orientation, PolicyType, ScrolledWindow, Spinner};
use std::cell::RefCell;
use std::rc::Rc;

pub fn build_ui(app: &adw::Application) {
    // Single-instance reuse: grid/launcher activation can deliver `activate`
    // more than once (startup + forwarded from the second process). Never
    // build a second window — present the existing one. windows() covers
    // presented-but-never-focused windows that active_window() misses.
    let windows = app.windows();
    if let Some(window) = windows.first() {
        window.present();
        return;
    }

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Fedora Driver Manager")
        .default_width(850)
        .default_height(600)
        .build();

    let toast_overlay = ToastOverlay::new();
    let split_view = NavigationSplitView::new();
    split_view.set_min_sidebar_width(220.0);
    split_view.set_max_sidebar_width(320.0);
    split_view.set_sidebar_width_fraction(0.30);

    // --- SIDEBAR ---
    let sidebar_page = NavigationPage::builder()
        .title("Categories")
        .build();

    let sidebar_box = Box::new(Orientation::Vertical, 6);
    sidebar_box.set_width_request(240);

    let sidebar_header = HeaderBar::new();
    sidebar_header.set_title_widget(Some(&WindowTitle::new("Driver Manager", "Fedora Workstation")));
    sidebar_box.append(&sidebar_header);

    let sidebar_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::Single)
        .css_classes(["navigation-sidebar"])
        .build();

    let categories = [
        ("overview", "Overview", "view-dashboard-symbolic"),
        ("gpu", "Graphics and Display", "video-display-symbolic"),
        ("network", "Network and WiFi", "network-wireless-symbolic"),
        ("audio", "Audio and Sound", "audio-card-symbolic"),
        ("storage", "Storage and NVMe", "drive-harddisk-symbolic"),
        ("peripherals", "Camera and Peripherals", "camera-web-symbolic"),
        ("other", "Other / System Devices", "preferences-system-devices-symbolic"),
        ("firmware", "Firmware Updates", "software-update-available-symbolic"),
        ("history", "History and Rollback", "document-revert-symbolic"),
        ("drivers", "Available Drivers", "package-x-generic-symbolic"),
    ];

    for (id, label, icon) in &categories {
        let row = ActionRow::builder()
            .title(*label)
            .use_markup(false)
            .build();
        let prefix_icon = Image::from_icon_name(icon);
        prefix_icon.set_pixel_size(16);
        row.add_prefix(&prefix_icon);
        row.set_widget_name(id);
        sidebar_list.append(&row);
    }

    let sidebar_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .child(&sidebar_list)
        .vexpand(true)
        .build();
    sidebar_box.append(&sidebar_scroll);
    sidebar_page.set_child(Some(&sidebar_box));
    split_view.set_sidebar(Some(&sidebar_page));

    // --- CONTENT STACK ---
    let content_page = NavigationPage::builder()
        .title("Hardware Details")
        .build();

    let content_box = Box::new(Orientation::Vertical, 0);
    let content_header = HeaderBar::new();
    let header_title = WindowTitle::new("Overview", "System hardware & drivers");
    content_header.set_title_widget(Some(&header_title));

    let refresh_btn = Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Rescan Hardware")
        .build();
    content_header.pack_end(&refresh_btn);

    let spinner = Spinner::new();
    spinner.set_tooltip_text(Some("Scanning hardware…"));
    content_header.pack_end(&spinner);

    let menu_btn = gtk4::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Main Menu")
        .build();
    let menu = gio::Menu::new();
    menu.append(Some("What's New"), Some("win.whatsnew"));
    menu.append(Some("About Driver Manager"), Some("win.about"));
    menu_btn.set_menu_model(Some(&menu));
    content_header.pack_end(&menu_btn);
    content_box.append(&content_header);

    let stack = gtk4::Stack::builder()
        .transition_type(gtk4::StackTransitionType::Crossfade)
        .vexpand(true)
        .build();

    content_box.append(&stack);
    content_page.set_child(Some(&content_box));
    split_view.set_content(Some(&content_page));

    toast_overlay.set_child(Some(&split_view));
    window.set_content(Some(&toast_overlay));

    // Shared state
    let devices_state = Rc::new(RefCell::new(Vec::<HardwareDevice>::new()));
    let toast_ref = Rc::new(toast_overlay);

    // Initial population (async: scans run off the main thread)
    refresh_data_async(
        stack.clone(),
        devices_state.clone(),
        toast_ref.clone(),
        spinner.clone(),
        refresh_btn.clone(),
        sidebar_list.clone(),
        false,
    );

    // Sidebar selection handler
    let stack_clone = stack.clone();
    let header_title_clone = header_title.clone();
    sidebar_list.connect_row_selected(move |_, row| {
        if let Some(r) = row {
            let name = r.widget_name();
            // During async (re)loads the stack may not have the page yet:
            // only switch when it exists.
            if stack_clone.child_by_name(&name).is_some() {
                stack_clone.set_visible_child_name(&name);
            }

            // Update title
            match name.as_str() {
                "overview" => header_title_clone.set_title("Overview"),
                "gpu" => header_title_clone.set_title("Graphics and Display"),
                "network" => header_title_clone.set_title("Network and WiFi"),
                "audio" => header_title_clone.set_title("Audio and Sound"),
                "storage" => header_title_clone.set_title("Storage and NVMe"),
                "peripherals" => header_title_clone.set_title("Camera and Peripherals"),
                "other" => header_title_clone.set_title("Other / System Devices"),
                "firmware" => header_title_clone.set_title("Firmware Updates"),
                "history" => header_title_clone.set_title("History and Rollback"),
                "drivers" => header_title_clone.set_title("Available Drivers"),
                _ => header_title_clone.set_title("Hardware Details"),
            }
        }
    });

    // Refresh button handler (async re-scan; success toast gated on clean scan)
    let stack_clone2 = stack.clone();
    let devices_clone2 = devices_state.clone();
    let toast_clone2 = toast_ref.clone();
    let spinner_clone2 = spinner.clone();
    let refresh_clone2 = refresh_btn.clone();
    let sidebar_clone2 = sidebar_list.clone();
    refresh_btn.connect_clicked(move |_| {
        refresh_data_async(
            stack_clone2.clone(),
            devices_clone2.clone(),
            toast_clone2.clone(),
            spinner_clone2.clone(),
            refresh_clone2.clone(),
            sidebar_clone2.clone(),
            true,
        );
    });

    // About action (window-scoped): shows app version, developer, license.
    let about_action = gio::SimpleAction::new("about", None);
    {
        let window_clone = window.clone();
        about_action.connect_activate(move |_, _| {
            let dialog = adw::AboutDialog::builder()
                .application_name("Driver Manager")
                .application_icon("io.github.shridhargsabat.DriverManager")
                .developer_name("Shridhar Sabat")
                .version(env!("CARGO_PKG_VERSION"))
                .website("https://github.com/shridhargsabat/fedora-driver-manager")
                .issue_url("https://github.com/shridhargsabat/fedora-driver-manager/issues")
                .license_type(gtk4::License::Gpl30)
                .build();
            dialog.present(Some(&window_clone));
        });
    }
    window.add_action(&about_action);

    // What's New action (window-scoped): release notes dialog next to About.
    let whatsnew_action = gio::SimpleAction::new("whatsnew", None);
    {
        let window_clone = window.clone();
        whatsnew_action.connect_activate(move |_, _| {
            show_whatsnew_dialog(&window_clone);
        });
    }
    window.add_action(&whatsnew_action);

    window.present();
}

/// Blocking scan results, collected on a worker thread so the GTK main
/// thread stays responsive. All fields are plain data (`Send`), and backend
/// errors travel as pre-rendered strings.
struct ScanResults {
    report: ScanReport,
    driver_packages: Result<Vec<DriverPackage>, String>,
    pending_updates: Result<Vec<PackageUpdate>, String>,
    firmware: Result<Vec<FirmwareItem>, String>,
    history: Result<Vec<DnfTransaction>, String>,
    laptop_model: String,
    secure_boot: String,
}

impl ScanResults {
    fn collect() -> Self {
        let report = HardwareScanner::scan_all_report();
        let candidates = HardwareScanner::collect_package_candidates(&report.devices);
        let candidate_refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
        let driver_packages =
            SystemBackend::get_driver_packages(&candidate_refs).map_err(|e| format!("{e:#}"));
        let pending_updates = match &driver_packages {
            Ok(pkgs) => {
                let names: Vec<&str> = candidates.iter().map(String::as_str).collect();
                SystemBackend::get_pending_updates(&names, pkgs).map_err(|e| format!("{e:#}"))
            }
            Err(e) => Err(format!("driver query failed: {e}")),
        };
        Self {
            report,
            driver_packages,
            pending_updates,
            firmware: SystemBackend::get_firmware_devices().map_err(|e| format!("{e:#}")),
            history: SystemBackend::get_dnf_history(10).map_err(|e| format!("{e:#}")),
            laptop_model: SystemBackend::get_laptop_model(),
            secure_boot: SystemBackend::get_secure_boot_state(),
        }
    }
}

/// Re-scan hardware off the main thread: spin the header spinner, collect
/// `ScanResults` via `gio::spawn_blocking`, then render back on the main
/// thread (toasts included). The Refresh button is desensitized while a scan
/// is in flight so scans cannot overlap.
fn refresh_data_async(
    stack: gtk4::Stack,
    devices_state: Rc<RefCell<Vec<HardwareDevice>>>,
    toast_overlay: Rc<ToastOverlay>,
    spinner: Spinner,
    refresh_btn: Button,
    sidebar: gtk4::ListBox,
    success_toast: bool,
) {
    spinner.start();
    refresh_btn.set_sensitive(false);
    glib::spawn_future_local(async move {
        let results = match gio::spawn_blocking(ScanResults::collect).await {
            Ok(results) => results,
            Err(_) => {
                toast_overlay.add_toast(Toast::new("Hardware scan task did not complete"));
                spinner.stop();
                refresh_btn.set_sensitive(true);
                return;
            }
        };
        // Gate the success toast on a genuinely clean scan: every failure
        // path already produced its own error toast in render_data.
        let clean = results.report.errors().is_empty()
            && results.firmware.is_ok()
            && results.history.is_ok()
            && results.driver_packages.is_ok()
            && results.pending_updates.is_ok();
        render_data(&stack, &devices_state, &toast_overlay, results);
        // Rebuilds reset the stack to its first child: restore whatever page
        // the sidebar has selected (also covers clicks made mid-scan).
        if let Some(selected) = sidebar.selected_row() {
            let name = selected.widget_name();
            if stack.child_by_name(&name).is_some() {
                stack.set_visible_child_name(&name);
            }
        }
        if success_toast && clean {
            toast_overlay.add_toast(Toast::new("Hardware re-scanned successfully"));
        }
        spinner.stop();
        refresh_btn.set_sensitive(true);
    });
}

/// Health-bucket counts for the overview subtitle: (optimal, actionable,
/// unknown). Anything neither optimal nor actionable (Unknown,
/// FirmwareUpdate, GenericFallback) lands in the third bucket so the
/// subtitle always sums to the device total.
fn health_counts(devices: &[HardwareDevice]) -> (usize, usize, usize) {
    let optimal = devices.iter().filter(|d| d.status == DriverStatus::Optimal).count();
    let actionable = devices
        .iter()
        .filter(|d| matches!(d.status, DriverStatus::Upgradable | DriverStatus::Missing))
        .count();
    let unknown = devices.len() - optimal - actionable;
    (optimal, actionable, unknown)
}

/// Total pending updates: package upgrades + updatable firmware items.
fn update_count(pkgs: &[PackageUpdate], firmware: &[FirmwareItem]) -> usize {
    pkgs.len() + firmware.iter().filter(|f| f.updatable).count()
}

/// Overview health subtitle, e.g. "23 optimal, 0 actionable, 2 unknown".
/// `suffix` carries scan caveats such as " (partial scan failure)".
fn health_subtitle(optimal: usize, actionable: usize, unknown: usize, suffix: &str) -> String {
    format!("{optimal} optimal, {actionable} actionable, {unknown} unknown{suffix}")
}

/// Where to get a package that is in no enabled repo. Only packages the
/// app itself recommends are listed — anything else yields None.
fn repo_hint(package: &str) -> Option<&'static str> {
    match package {
        "akmod-nvidia" | "broadcom-wl" => Some("Requires the RPM Fusion repositories"),
        _ => None,
    }
}

fn render_data(
    stack: &gtk4::Stack,
    devices_state: &Rc<RefCell<Vec<HardwareDevice>>>,
    toast_overlay: &Rc<ToastOverlay>,
    results: ScanResults,
) {
    let report = results.report;
    let scan_errors = report.errors();
    let scan_error: Option<String> = report
        .all_backends_failed()
        .then(|| scan_errors.join("; "));
    let scan_warnings = report.warnings.clone();
    let scanned: Vec<HardwareDevice> = report.devices;
    *devices_state.borrow_mut() = scanned.clone();

    for err in &scan_errors {
        toast_overlay.add_toast(Toast::new(err));
    }
    for warning in &scan_warnings {
        toast_overlay.add_toast(Toast::new(warning));
    }

    // Drop existing stack pages first: re-adding the same child names is
    // rejected by GtkStack ("Duplicate child name"), which made Refresh a
    // silent no-op. Clear everything, then rebuild below.
    while let Some(child) = stack.first_child() {
        stack.remove(&child);
    }

    // 1. Overview Page
    let overview_page = PreferencesPage::new();
    // PreferencesGroup description parses Pango markup (and offers no
    // use-markup toggle), so escape the sysfs-derived model string.
    let sys_group = PreferencesGroup::builder()
        .title("System Overview")
        .description(format!(
            "Device: {}",
            glib::markup_escape_text(&results.laptop_model)
        ))
        .build();

    let sb_row = ActionRow::builder()
        .title(results.secure_boot.as_str())
        .subtitle("Kernel module signature verification")
        .build();
    let sb_icon = Image::from_icon_name("security-high-symbolic");
    sb_icon.set_pixel_size(16);
    sb_row.add_prefix(&sb_icon);
    sys_group.add(&sb_row);

    let count_row = ActionRow::builder()
        .title(format!("Total Hardware Devices Detected: {}", scanned.len()))
        .subtitle("PCIe, USB, input and bridge devices")
        .build();
    let count_icon = Image::from_icon_name("preferences-system-devices-symbolic");
    count_icon.set_pixel_size(16);
    count_row.add_prefix(&count_icon);
    sys_group.add(&count_row);
    overview_page.add(&sys_group);

    // Add quick health summary
    let health_group = PreferencesGroup::builder()
        .title("Driver Health Status")
        .build();

    let (optimal_count, action_count, unknown_count) = health_counts(&scanned);

    let (health_title, health_subtitle, health_icon_name) = if scanned.is_empty() {
        // A 0-device scan (missing lspci/lsusb, empty output) means we know
        // nothing about the hardware: never report "optimal" here.
        match &scan_error {
            Some(err) => (
                "Hardware scan failed — status unknown".to_string(),
                format!("Could not enumerate devices: {err}"),
                "dialog-question-symbolic",
            ),
            None => (
                "No hardware devices detected — status unknown".to_string(),
                "lspci/lsusb returned no devices; status cannot be determined".to_string(),
                "dialog-question-symbolic",
            ),
        }
    } else if action_count == 0 && scan_errors.is_empty() && scan_warnings.is_empty() {
        (
            "All devices operating optimally".to_string(),
            health_subtitle(optimal_count, action_count, unknown_count, ""),
            "emblem-ok-symbolic",
        )
    } else {
        // Partial scan failures and format warnings count against Optimal:
        // part of the hardware state is Unknown.
        let mut notes = Vec::new();
        if !scan_errors.is_empty() {
            notes.push("partial scan failure");
        }
        if !scan_warnings.is_empty() {
            notes.push("output format warnings");
        }
        let suffix = if notes.is_empty() {
            String::new()
        } else {
            format!(" ({})", notes.join(", "))
        };
        (
            "Updates or attention recommended".to_string(),
            health_subtitle(optimal_count, action_count, unknown_count, &suffix),
            "dialog-warning-symbolic",
        )
    };

    let health_row = ActionRow::builder()
        .title(&health_title)
        .subtitle(&health_subtitle)
        .use_markup(false)
        .build();
    let health_icon = Image::from_icon_name(health_icon_name);
    health_icon.set_pixel_size(16);
    health_row.add_prefix(&health_icon);
    health_group.add(&health_row);
    overview_page.add(&health_group);

    stack.add_named(&wrap_in_scrolled(overview_page), Some("overview"));

    // 2. Hardware Category Pages
    add_category_page(stack, &scanned, DeviceCategory::Gpu, "gpu", toast_overlay);
    add_category_page(stack, &scanned, DeviceCategory::Network, "network", toast_overlay);
    add_category_page(stack, &scanned, DeviceCategory::Audio, "audio", toast_overlay);
    add_category_page(stack, &scanned, DeviceCategory::Storage, "storage", toast_overlay);

    // Peripherals (Camera, Input, Thunderbolt)
    let peri_page = PreferencesPage::new();
    let peri_group = PreferencesGroup::builder()
        .title("Peripherals &amp; Connected Devices")
        .build();

    let peri_devices: Vec<_> = scanned.iter().filter(|d| {
        matches!(d.category, DeviceCategory::Camera | DeviceCategory::Input | DeviceCategory::Thunderbolt)
    }).collect();

    for dev in peri_devices {
        peri_group.add(&build_device_row(dev, toast_overlay));
    }
    peri_page.add(&peri_group);
    stack.add_named(&wrap_in_scrolled(peri_page), Some("peripherals"));

    // Other / System Devices (bridges, serial controllers, SMBus, ...):
    // counted in the totals but previously rendered on no page.
    add_category_page(stack, &scanned, DeviceCategory::Other, "other", toast_overlay);

    // 3. Firmware Page
    let fw_page = PreferencesPage::new();
    let fw_group = PreferencesGroup::builder()
        .title("Firmware Devices (fwupd)")
        .description("Hardware firmware detected via Linux Vendor Firmware Service")
        .build();

    // Distinguish "tool failed" (error row + toast) from "genuine empty
    // result" ("none found" row) instead of rendering a silent empty group.
    let fw_result = results.firmware;
    if let Err(e) = &fw_result {
        let err_row = ActionRow::builder()
            .title("Firmware scan failed")
            .subtitle(format!("fwupdmgr error: {e}"))
            .use_markup(false)
            .build();
        let err_icon = Image::from_icon_name("dialog-error-symbolic");
        err_icon.set_pixel_size(16);
        err_row.add_prefix(&err_icon);
        fw_group.add(&err_row);
        toast_overlay.add_toast(Toast::new(&format!("Firmware scan failed: {e}")));
    }
    if let Ok(fw_devices) = &fw_result {
        for fw in fw_devices {
            let subtitle = match fw.summary.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(summary) => {
                    format!("Vendor: {} | Version: {}\n{}", fw.vendor, fw.version, summary)
                }
                None => format!("Vendor: {} | Version: {}", fw.vendor, fw.version),
            };
            let row = ActionRow::builder()
                .title(&fw.name)
                .subtitle(subtitle)
                .use_markup(false)
                .build();
            let fw_icon = Image::from_icon_name("system-software-update-symbolic");
            fw_icon.set_pixel_size(16);
            row.add_prefix(&fw_icon);

            if fw.updatable {
                let badge = Label::builder()
                    .label("Updatable")
                    .css_classes(["success", "heading"])
                    .valign(Align::Center)
                    .build();
                row.add_suffix(&badge);
            }
            fw_group.add(&row);
        }
        if fw_devices.is_empty() {
            let empty_row = ActionRow::builder()
                .title("No firmware devices found")
                .subtitle("fwupd reported no devices on this system")
                .build();
            let info_icon = Image::from_icon_name("dialog-information-symbolic");
            info_icon.set_pixel_size(16);
            empty_row.add_prefix(&info_icon);
            fw_group.add(&empty_row);
        }
    }
    fw_page.add(&fw_group);
    stack.add_named(&wrap_in_scrolled(fw_page), Some("firmware"));

    // 4. History & Rollback Page
    let hist_page = PreferencesPage::new();
    let hist_group = PreferencesGroup::builder()
        .title("Recent Package Transactions")
        .description("DNF history records available for undo / rollback")
        .build();

    let hist_result = results.history;
    if let Err(e) = &hist_result {
        let err_row = ActionRow::builder()
            .title("Transaction history unavailable")
            .subtitle(format!("dnf5 error: {e}"))
            .use_markup(false)
            .build();
        let err_icon = Image::from_icon_name("dialog-error-symbolic");
        err_icon.set_pixel_size(16);
        err_row.add_prefix(&err_icon);
        hist_group.add(&err_row);
        toast_overlay.add_toast(Toast::new(&format!("Transaction history unavailable: {e}")));
    }
    if let Ok(transactions) = &hist_result {
        for tx in transactions {
            let row = ActionRow::builder()
                .title(format!("Transaction #{}", tx.id))
                .subtitle(&tx.command_line)
                .use_markup(false)
                .build();
            let hist_icon = Image::from_icon_name("document-revert-symbolic");
            hist_icon.set_pixel_size(16);
            row.add_prefix(&hist_icon);

            let toast_c = toast_overlay.clone();
            let undo_btn = Button::builder()
                .label("Undo")
                .valign(Align::Center)
                .css_classes(["destructive-action"])
                .build();

            let tx_id = tx.id;
            let tx_cmd = tx.command_line.clone();
            undo_btn.connect_clicked(move |btn| {
                // Confirm before running a privileged undo: show what will run as root.
                let toast = toast_c.clone();
                let cmd = tx_cmd.clone();
                let parent = btn
                    .root()
                    .and_then(|root| root.downcast::<gtk4::Window>().ok());
                let dialog = adw::MessageDialog::new(
                    parent.as_ref(),
                    Some(&format!("Undo transaction #{}?", tx_id)),
                    Some(&format!(
                        "This will run the following command as root:\n\ndnf5 history undo -y {}\n\nOriginal command: {}",
                        tx_id, cmd
                    )),
                );
                dialog.add_responses(&[("cancel", "Cancel"), ("undo", "Undo")]);
                dialog.set_response_appearance("undo", adw::ResponseAppearance::Destructive);
                dialog.set_default_response(Some("cancel"));
                dialog.set_close_response("cancel");
                dialog.connect_response(None, move |_, response| {
                    if response == "undo" {
                        let toast = toast.clone();
                        glib::spawn_future_local(async move {
                            toast.add_toast(Toast::new(&format!("Requesting pkexec to undo transaction #{}...", tx_id)));
                            let res = SystemBackend::execute_root_async(RootAction::history_undo(tx_id)).await;
                            match res {
                                Ok(_) => toast.add_toast(Toast::new(&format!("Transaction #{} undone successfully", tx_id))),
                                Err(e) => toast.add_toast(Toast::new(&format!("Undo failed: {}", e))),
                            }
                        });
                    }
                });
                dialog.present();
            });

            row.add_suffix(&undo_btn);
            hist_group.add(&row);
        }
        if transactions.is_empty() {
            let empty_row = ActionRow::builder()
                .title("No package transactions found")
                .subtitle("dnf5 history is empty")
                .build();
            let info_icon = Image::from_icon_name("dialog-information-symbolic");
            info_icon.set_pixel_size(16);
            empty_row.add_prefix(&info_icon);
            hist_group.add(&empty_row);
        }
    }
    hist_page.add(&hist_group);
    stack.add_named(&wrap_in_scrolled(hist_page), Some("history"));

    add_drivers_page(stack, &results.driver_packages, toast_overlay);
}

fn add_category_page(
    stack: &gtk4::Stack,
    devices: &[HardwareDevice],
    category: DeviceCategory,
    name: &str,
    toast_overlay: &Rc<ToastOverlay>,
) {
    let page = PreferencesPage::new();
    let group = PreferencesGroup::builder()
        .title(category.display_name())
        .build();

    let matched: Vec<_> = devices.iter().filter(|d| d.category == category).collect();

    if matched.is_empty() {
        let empty_row = ActionRow::builder()
            .title("No devices found in this category")
            .build();
        let info_icon = Image::from_icon_name("dialog-information-symbolic");
        info_icon.set_pixel_size(16);
        empty_row.add_prefix(&info_icon);
        group.add(&empty_row);
    } else {
        for dev in matched {
            group.add(&build_device_row(dev, toast_overlay));
        }
    }

    page.add(&group);
    stack.add_named(&wrap_in_scrolled(page), Some(name));
}

/// "Available Drivers" page: one row per candidate package with its
/// installed/available state from the enabled dnf repos, plus an Install
/// button when there is something installable. Mirrors the firmware
/// page structure (PreferencesPage -> PreferencesGroup -> rows).
fn add_drivers_page(
    stack: &gtk4::Stack,
    packages: &Result<Vec<DriverPackage>, String>,
    toast_overlay: &Rc<ToastOverlay>,
) {
    let page = PreferencesPage::new();
    let group = PreferencesGroup::builder()
        .title("Official driver packages")
        .description("Versions from your enabled repositories")
        .build();
    match packages {
        Err(err) => {
            let row = ActionRow::builder()
                .title("Could not query repositories")
                .subtitle(err.as_str())
                .build();
            row.add_prefix(&Image::from_icon_name("dialog-error-symbolic"));
            group.add(&row);
            toast_overlay.add_toast(Toast::new(&format!("Driver query failed: {err}")));
        }
        Ok(list) if list.is_empty() => {
            let row = ActionRow::builder()
                .title("No driver packages suggested")
                .subtitle("No detected device maps to an installable package")
                .build();
            group.add(&row);
        }
        Ok(list) => {
            for pkg in list {
                // Parser/dnf-derived strings: never markup (file convention).
                let row = ActionRow::builder()
                    .title(pkg.name.as_str())
                    .subtitle(pkg.status_text().as_str())
                    .use_markup(false)
                    .build();
                row.add_prefix(&Image::from_icon_name("package-x-generic-symbolic"));
                if pkg.installed_evr.is_some() {
                    row.add_suffix(&Image::from_icon_name("emblem-ok-symbolic"));
                    row.add_suffix(&make_reinstall_button(&pkg.name, toast_overlay));
                } else if pkg.preferred_available().is_some() {
                    row.add_suffix(&make_install_button(&pkg.name, toast_overlay));
                } else if let Some(hint) = repo_hint(&pkg.name) {
                    let hint_row = ActionRow::builder().title(hint).build();
                    group.add(&row);
                    group.add(&hint_row);
                    continue;
                }
                group.add(&row);
            }
        }
    }
    page.add(&group);
    stack.add_named(&wrap_in_scrolled(page), Some("drivers"));
}

/// What's New dialog (header menu, next to About): the running version's
/// notes on the first page, older releases as history pages. Content
/// comes from embedded CHANGELOG.md; empty notes render a graceful
/// placeholder, never a blank dialog.
fn show_whatsnew_dialog(parent: &adw::ApplicationWindow) {
    let dialog = adw::PreferencesDialog::builder().title("What's New").build();
    let notes = SystemBackend::get_release_notes();
    let current = env!("CARGO_PKG_VERSION");
    if notes.is_empty() {
        let page = PreferencesPage::builder().title("Release notes").build();
        let group = PreferencesGroup::builder().title("Release notes").build();
        group.add(
            &ActionRow::builder()
                .title("No release notes available")
                .build(),
        );
        page.add(&group);
        dialog.add(&page);
    } else {
        for rel in &notes {
            let title = if rel.version == current {
                format!("{} ({}) — this version", rel.version, rel.date)
            } else {
                format!("{} ({})", rel.version, rel.date)
            };
            let page = PreferencesPage::builder().title(title.as_str()).build();
            let group = PreferencesGroup::builder().title(title.as_str()).build();
            if rel.bullets.is_empty() {
                group.add(&ActionRow::builder().title("No details recorded").build());
            }
            for bullet in &rel.bullets {
                // Changelog-derived strings: never markup (file convention).
                group.add(
                    &ActionRow::builder()
                        .title(bullet.as_str())
                        .use_markup(false)
                        .build(),
                );
            }
            page.add(&group);
            dialog.add(&page);
        }
    }
    dialog.present(Some(parent));
}

fn build_device_row(dev: &HardwareDevice, toast_overlay: &Rc<ToastOverlay>) -> ExpanderRow {
    // All titles/subtitles below carry parser data (lspci names, drivers,
    // module lists): keep markup off so device strings render literally.
    let expander = ExpanderRow::builder()
        .title(&dev.name)
        .subtitle(format!(
            "Driver: {} • Status: {}",
            dev.current_driver.as_deref().unwrap_or("None (Subsystem managed)"),
            dev.status.label()
        ))
        .use_markup(false)
        .build();

    let category_icon = Image::from_icon_name(dev.category.icon_name());
    category_icon.set_pixel_size(16);
    expander.add_prefix(&category_icon);

    let status_icon = Image::from_icon_name(dev.status.icon());
    status_icon.set_pixel_size(16);
    expander.add_suffix(&status_icon);

    if let Some(ref rec) = dev.recommendation {
        let rec_row = ActionRow::builder()
            .title("Recommendation")
            .subtitle(rec)
            .use_markup(false)
            .build();
        let rec_icon = Image::from_icon_name("dialog-information-symbolic");
        rec_icon.set_pixel_size(16);
        rec_row.add_prefix(&rec_icon);
        expander.add_row(&rec_row);
    }

    let id_row = ActionRow::builder()
        .title("Hardware Identifier")
        .subtitle(format!("Vendor: [0x{}] Device: [0x{}] Slot: {}", dev.vendor_id, dev.device_id, dev.id))
        .use_markup(false)
        .build();
    let id_icon = Image::from_icon_name("preferences-system-details-symbolic");
    id_icon.set_pixel_size(16);
    id_row.add_prefix(&id_icon);
    expander.add_row(&id_row);

    if !dev.available_modules.is_empty() {
        let mod_row = ActionRow::builder()
            .title("Compatible Kernel Modules")
            .subtitle(dev.available_modules.join(", "))
            .use_markup(false)
            .build();
        let mod_icon = Image::from_icon_name("system-run-symbolic");
        mod_icon.set_pixel_size(16);
        mod_row.add_prefix(&mod_icon);
        expander.add_row(&mod_row);
    }

    // Action button if actionable package candidate exists
    if let Some(ref pkg) = dev.package_candidate {
        expander.add_suffix(&make_install_button(pkg, toast_overlay));
    }

    expander
}

/// "Install {package}" button with destructive confirmation dialog.
/// Shared by device rows and the Available Drivers page; the flow is:
/// confirm -> `RootAction::dnf_install` validation -> `execute_root_async` -> toast.
fn make_install_button(package: &str, toast_overlay: &Rc<ToastOverlay>) -> Button {
    make_package_action_button(package, toast_overlay, false)
}

fn make_reinstall_button(package: &str, toast_overlay: &Rc<ToastOverlay>) -> Button {
    make_package_action_button(package, toast_overlay, true)
}

/// Install/Reinstall button with destructive confirmation dialog.
/// `reinstall` selects `dnf5 reinstall -y` (repair) instead of install;
/// validation, confirm dialog, and toasts are otherwise identical.
fn make_package_action_button(
    package: &str,
    toast_overlay: &Rc<ToastOverlay>,
    reinstall: bool,
) -> Button {
    let verb = if reinstall { "Reinstall" } else { "Install" };
    let dnf_cmd = if reinstall { "reinstall" } else { "install" };
    let done_word = if reinstall { "reinstalled" } else { "installed" };
    // The button runs `dnf5 {install,reinstall} -y <pkg>` as root: label it honestly.
    let btn_label = format!("{verb} {package}");

    let action_btn = Button::builder()
        .label(&btn_label)
        .valign(Align::Center)
        .css_classes(["suggested-action"])
        .build();

    let pkg_clone = package.to_string();
    let toast_c = toast_overlay.clone();
    action_btn.connect_clicked(move |btn| {
        // Confirm before running a privileged action: show what will run as root.
        let toast_c = toast_c.clone();
        let p = pkg_clone.clone();
        let parent = btn
            .root()
            .and_then(|root| root.downcast::<gtk4::Window>().ok());
        let dialog = adw::MessageDialog::new(
            parent.as_ref(),
            Some(&format!("{verb} {p}?")),
            Some(&format!(
                "This will run the following command as root:\n\ndnf5 {dnf_cmd} -y {p}"
            )),
        );
        dialog.add_responses(&[("cancel", "Cancel"), ("install", verb)]);
        dialog.set_response_appearance("install", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.connect_response(None, move |_, response| {
            if response == "install" {
                let toast = toast_c.clone();
                let p = p.clone();
                // Validate before pkexec: malicious names never reach root.
                let action = if reinstall {
                    RootAction::dnf_reinstall(&p)
                } else {
                    RootAction::dnf_install(&p)
                };
                match action {
                    Ok(action) => {
                        glib::spawn_future_local(async move {
                            toast.add_toast(Toast::new(&format!("Running dnf5 {dnf_cmd} for {}...", p)));
                            let res = SystemBackend::execute_root_async(action).await;
                            match res {
                                Ok(_) => toast.add_toast(Toast::new(&format!(
                                    "{} {} successfully!",
                                    p, done_word
                                ))),
                                Err(e) => toast.add_toast(Toast::new(&format!("Failed: {}", e))),
                            }
                        });
                    }
                    Err(e) => {
                        toast.add_toast(Toast::new(&format!("Refusing privileged action on '{}': {}", p, e)));
                    }
                }
            }
        });
        dialog.present();
    });

    action_btn
}

fn wrap_in_scrolled(widget: impl IsA<gtk4::Widget>) -> ScrolledWindow {
    ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .child(&widget)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device_with_status(id: &str, status: DriverStatus) -> HardwareDevice {
        HardwareDevice {
            id: id.to_string(),
            name: format!("Test device {id}"),
            vendor: "Test Vendor".to_string(),
            vendor_id: "1234".to_string(),
            device_id: "5678".to_string(),
            subsystem: None,
            category: DeviceCategory::Other,
            current_driver: None,
            available_modules: Vec::new(),
            status,
            recommendation: None,
            package_candidate: None,
        }
    }

    #[test]
    fn health_subtitle_includes_unknown_count() {
        let devices = vec![
            device_with_status("a", DriverStatus::Optimal),
            device_with_status("b", DriverStatus::Missing),
            device_with_status("c", DriverStatus::Unknown),
        ];
        let (optimal, actionable, unknown) = health_counts(&devices);
        assert_eq!((optimal, actionable, unknown), (1, 1, 1));
        assert_eq!(
            health_subtitle(optimal, actionable, unknown, ""),
            "1 optimal, 1 actionable, 1 unknown"
        );
    }

    #[test]
    fn health_subtitle_keeps_suffix_after_unknown() {
        assert_eq!(
            health_subtitle(23, 0, 2, " (partial scan failure)"),
            "23 optimal, 0 actionable, 2 unknown (partial scan failure)"
        );
    }

    #[test]
    fn rpm_fusion_packages_get_repo_hint() {
        assert_eq!(repo_hint("akmod-nvidia"), Some("Requires the RPM Fusion repositories"));
        assert_eq!(repo_hint("broadcom-wl"), Some("Requires the RPM Fusion repositories"));
    }

    #[test]
    fn official_packages_get_no_hint() {
        assert_eq!(repo_hint("libva-intel-media-driver"), None);
        assert_eq!(repo_hint("something-else"), None);
    }

    #[test]
    fn update_count_sums_packages_and_firmware() {
        let pkgs = vec![PackageUpdate {
            name: "a".to_string(), arch: "x86_64".to_string(),
            from_evr: "1".to_string(), to_evr: "2".to_string(), repo: "updates".to_string(),
        }];
        let fw = vec![
            FirmwareItem { name: "BIOS".to_string(), vendor: "Intel".to_string(), version: "1".to_string(), updatable: true, summary: None },
            FirmwareItem { name: "TPM".to_string(), vendor: "Intel".to_string(), version: "1".to_string(), updatable: false, summary: None },
        ];
        assert_eq!(update_count(&pkgs, &fw), 2);
    }
}
