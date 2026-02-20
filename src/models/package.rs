use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct PackageSearchResponse {
    pub total_count: i64,
    pub results: Vec<Package>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct Package {
    pub id: String,
    pub package_name: String,
    pub file_name: String,
    pub category_id: String,
    pub priority: i32,
    pub fill_user_template: bool,
    pub fill_existing_users: bool,
    pub reboot_required: bool,
    pub os_install: bool,
    pub suppress_updates: bool,
    pub suppress_from_dock: bool,
    pub suppress_eula: bool,
    pub suppress_registration: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageCreateRequest {
    pub package_name: String,
    pub file_name: String,
    pub category_id: String,
    pub priority: i32,
    pub fill_user_template: bool,
    pub fill_existing_users: bool,
    pub reboot_required: bool,
    pub os_install: bool,
    pub suppress_updates: bool,
    pub suppress_from_dock: bool,
    pub suppress_eula: bool,
    pub suppress_registration: bool,
}

impl PackageCreateRequest {
    pub fn new_default(package_name: &str, file_name: &str) -> Self {
        Self {
            package_name: package_name.to_string(),
            file_name: file_name.to_string(),
            category_id: "-1".to_string(),
            priority: 3,
            fill_user_template: false,
            fill_existing_users: false,
            reboot_required: false,
            os_install: false,
            suppress_updates: false,
            suppress_from_dock: false,
            suppress_eula: false,
            suppress_registration: false,
        }
    }

    pub fn from_old(old: &Package, new_file_name: &str) -> Self {
        Self {
            package_name: old.package_name.clone(),
            file_name: new_file_name.to_string(),
            category_id: old.category_id.clone(),
            priority: old.priority,
            fill_user_template: old.fill_user_template,
            fill_existing_users: old.fill_existing_users,
            reboot_required: old.reboot_required,
            os_install: old.os_install,
            suppress_updates: old.suppress_updates,
            suppress_from_dock: old.suppress_from_dock,
            suppress_eula: old.suppress_eula,
            suppress_registration: old.suppress_registration,
        }
    }
}
