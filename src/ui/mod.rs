use crate::backend::hardware::HardwareScanner;
use crate::backend::system::SystemBackend;
use crate::models::{DeviceCategory, DriverStatus, HardwareDevice};
use adw::prelude::*;
use adw::{ActionRow, ExpanderRow, HeaderBar, NavigationPage, NavigationSplitView, PreferencesGroup, PreferencesPage, Toast, ToastOverlay, WindowTitle};
use gtk4::prelude::{IsA, WidgetExt};
use gtk4::{Align, Box, Button, Image, Label, Orientation, PolicyType, ScrolledWindow};
use std::cell::RefCell;
use std::rc::Rc;

pub fn build_ui(app: &adw::Application) {
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
    ];

    for (id, label, icon) in &categories {
        let row = ActionRow::builder()
            .title(*label)
            .use_markup(false)
            .build();
        let prefix_icon = Image::from_icon_name(*icon);
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

    // Initial population
    refresh_data(
        &stack,
        &devices_state,
        &toast_ref,
    );

    // Sidebar selection handler
    let stack_clone = stack.clone();
    let header_title_clone = header_title.clone();
    sidebar_list.connect_row_selected(move |_, row| {
        if let Some(r) = row {
            let name = r.widget_name();
            stack_clone.set_visible_child_name(&name);

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
                _ => header_title_clone.set_title("Hardware Details"),
            }
        }
    });

    // Refresh button handler
    let stack_clone2 = stack.clone();
    let devices_clone2 = devices_state.clone();
    let toast_clone2 = toast_ref.clone();
    refresh_btn.connect_clicked(move |_| {
        refresh_data(&stack_clone2, &devices_clone2, &toast_clone2);
        toast_clone2.add_toast(Toast::new("Hardware re-scanned successfully"));
    });

    window.present();
}

fn refresh_data(
    stack: &gtk4::Stack,
    devices_state: &Rc<RefCell<Vec<HardwareDevice>>>,
    toast_overlay: &Rc<ToastOverlay>,
) {
    let report = HardwareScanner::scan_all_report();
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
            glib::markup_escape_text(&SystemBackend::get_laptop_model())
        ))
        .build();

    let sb_row = ActionRow::builder()
        .title(SystemBackend::get_secure_boot_state())
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

    let optimal_count = scanned.iter().filter(|d| d.status == DriverStatus::Optimal).count();
    let action_count = scanned.iter().filter(|d| d.status == DriverStatus::Upgradable || d.status == DriverStatus::Missing).count();

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
            format!("{optimal_count} optimal, {action_count} actionable"),
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
            format!("{optimal_count} optimal, {action_count} actionable{suffix}"),
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
    let fw_result = SystemBackend::get_firmware_devices();
    if let Err(e) = &fw_result {
        let err_row = ActionRow::builder()
            .title("Firmware scan failed")
            .subtitle(format!("fwupdmgr error: {e:#}"))
            .use_markup(false)
            .build();
        let err_icon = Image::from_icon_name("dialog-error-symbolic");
        err_icon.set_pixel_size(16);
        err_row.add_prefix(&err_icon);
        fw_group.add(&err_row);
        toast_overlay.add_toast(Toast::new(&format!("Firmware scan failed: {e:#}")));
    }
    if let Ok(fw_devices) = &fw_result {
        for fw in fw_devices {
            let row = ActionRow::builder()
                .title(&fw.name)
                .subtitle(&format!("Vendor: {} | Version: {}", fw.vendor, fw.version))
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

    let hist_result = SystemBackend::get_dnf_history(10);
    if let Err(e) = &hist_result {
        let err_row = ActionRow::builder()
            .title("Transaction history unavailable")
            .subtitle(format!("dnf5 error: {e:#}"))
            .use_markup(false)
            .build();
        let err_icon = Image::from_icon_name("dialog-error-symbolic");
        err_icon.set_pixel_size(16);
        err_row.add_prefix(&err_icon);
        hist_group.add(&err_row);
        toast_overlay.add_toast(Toast::new(&format!("Transaction history unavailable: {e:#}")));
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
                            let res = SystemBackend::execute_pkexec_async(vec![
                                "dnf5".to_string(),
                                "history".to_string(),
                                "undo".to_string(),
                                "-y".to_string(),
                                tx_id.to_string(),
                            ])
                            .await;
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

fn build_device_row(dev: &HardwareDevice, toast_overlay: &Rc<ToastOverlay>) -> ExpanderRow {
    // All titles/subtitles below carry parser data (lspci names, drivers,
    // module lists): keep markup off so device strings render literally.
    let expander = ExpanderRow::builder()
        .title(&dev.name)
        .subtitle(&format!(
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
        .subtitle(&format!("Vendor: [0x{}] Device: [0x{}] Slot: {}", dev.vendor_id, dev.device_id, dev.id))
        .use_markup(false)
        .build();
    let id_icon = Image::from_icon_name("preferences-system-details-symbolic");
    id_icon.set_pixel_size(16);
    id_row.add_prefix(&id_icon);
    expander.add_row(&id_row);

    if !dev.available_modules.is_empty() {
        let mod_row = ActionRow::builder()
            .title("Compatible Kernel Modules")
            .subtitle(&dev.available_modules.join(", "))
            .use_markup(false)
            .build();
        let mod_icon = Image::from_icon_name("system-run-symbolic");
        mod_icon.set_pixel_size(16);
        mod_row.add_prefix(&mod_icon);
        expander.add_row(&mod_row);
    }

    // Action button if actionable package candidate exists
    if let Some(ref pkg) = dev.package_candidate {
        // The button runs `dnf5 install -y <pkg>` as root: label it honestly.
        let btn_label = format!("Install {pkg}");

        let action_btn = Button::builder()
            .label(&btn_label)
            .valign(Align::Center)
            .css_classes(["suggested-action"])
            .build();

        let pkg_clone = pkg.clone();
        let toast_c = toast_overlay.clone();
        action_btn.connect_clicked(move |_| {
            let toast = toast_c.clone();
            let p = pkg_clone.clone();
            glib::spawn_future_local(async move {
                toast.add_toast(Toast::new(&format!("Running dnf5 install for {}...", p)));
                let res = SystemBackend::execute_pkexec_async(vec![
                    "dnf5".to_string(),
                    "install".to_string(),
                    "-y".to_string(),
                    p.clone(),
                ])
                .await;
                match res {
                    Ok(_) => toast.add_toast(Toast::new(&format!("{} installed successfully!", p))),
                    Err(e) => toast.add_toast(Toast::new(&format!("Failed: {}", e))),
                }
            });
        });

        expander.add_suffix(&action_btn);
    }

    expander
}

fn wrap_in_scrolled(widget: impl IsA<gtk4::Widget>) -> ScrolledWindow {
    ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .vscrollbar_policy(PolicyType::Automatic)
        .child(&widget)
        .build()
}
