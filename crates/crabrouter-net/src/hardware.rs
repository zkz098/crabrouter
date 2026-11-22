use std::fs;
use std::path::Path;

use chrono::Utc;
use crabrouter_model::{
    DiskHealthReport, HardwareAlert, HardwareHealthSnapshot, HealthSeverity, ThermalSensorReading,
};
use serde_json::Value;
use tokio::process::Command;

use crate::Result;

/// Collects hardware health telemetry from `lm-sensors` (`sensors -j` / `/sys/class/hwmon`),
/// `smartctl -j -a`, and `nvme smart-log -o json`, then evaluates warning and critical alerts.
pub async fn collect_hardware_health(dry_run: bool) -> Result<HardwareHealthSnapshot> {
    let mut sensors = collect_thermal_sensors().await;
    let mut disks = collect_disk_health().await;

    if dry_run && sensors.is_empty() {
        sensors = sample_dry_run_sensors();
    }
    if dry_run && disks.is_empty() {
        disks = sample_dry_run_disks();
    }

    let (overall_severity, alerts) = evaluate_hardware_alerts(&sensors, &disks);

    Ok(HardwareHealthSnapshot {
        collected_at: Utc::now().to_rfc3339(),
        overall_severity,
        sensors,
        disks,
        alerts,
    })
}

async fn collect_thermal_sensors() -> Vec<ThermalSensorReading> {
    let mut cmd = Command::new("sensors");
    cmd.arg("-j");
    if let Ok(out) =
        crate::cmd::run_output_with_timeout(&mut cmd, std::time::Duration::from_secs(5), "sensors")
            .await
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        let parsed = parse_sensors_json(&text);
        if !parsed.is_empty() {
            return parsed;
        }
    }
    read_sysfs_hwmon_sensors()
}

/// Parses `sensors -j` JSON output from `lm-sensors`.
#[allow(clippy::cast_possible_truncation)]
pub fn parse_sensors_json(raw: &str) -> Vec<ThermalSensorReading> {
    let Ok(root) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let Some(chips) = root.as_object() else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for (chip_name, chip_val) in chips {
        let Some(features) = chip_val.as_object() else {
            continue;
        };
        for (feature_label, feature_val) in features {
            if feature_label == "Adapter" {
                continue;
            }
            let Some(sub_map) = feature_val.as_object() else {
                continue;
            };
            let mut input_c: Option<f32> = None;
            let mut high_c: Option<f32> = None;
            let mut crit_c: Option<f32> = None;

            for (sub_k, sub_v) in sub_map {
                let Some(val) = sub_v.as_f64() else {
                    continue;
                };
                if sub_k.starts_with("temp") && sub_k.ends_with("_input") {
                    input_c = Some(val as f32);
                } else if sub_k.starts_with("temp") && sub_k.ends_with("_max") {
                    high_c = Some(val as f32);
                } else if sub_k.starts_with("temp") && sub_k.ends_with("_crit") {
                    crit_c = Some(val as f32);
                }
            }

            if let Some(temp_celsius) = input_c
                && temp_celsius > -50.0
                && temp_celsius < 200.0
            {
                let severity = classify_temp_severity(temp_celsius, high_c, crit_c);
                out.push(ThermalSensorReading {
                    chip: chip_name.clone(),
                    label: feature_label.clone(),
                    temp_celsius,
                    high_celsius: high_c,
                    crit_celsius: crit_c,
                    severity,
                });
            }
        }
    }
    out
}

#[allow(clippy::cast_precision_loss)]
fn read_sysfs_hwmon_sensors() -> Vec<ThermalSensorReading> {
    let mut out = Vec::new();
    let hwmon_dir = Path::new("/sys/class/hwmon");
    if let Ok(entries) = fs::read_dir(hwmon_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let chip = fs::read_to_string(path.join("name"))
                .map_or_else(|_| "hwmon".to_string(), |s| s.trim().to_string());
            for idx in 1..=16 {
                let input_path = path.join(format!("temp{idx}_input"));
                let Ok(raw_temp) = fs::read_to_string(&input_path) else {
                    continue;
                };
                let Ok(milli) = raw_temp.trim().parse::<i32>() else {
                    continue;
                };
                if milli <= 0 {
                    continue;
                }
                let temp_celsius = milli as f32 / 1000.0;
                let label = fs::read_to_string(path.join(format!("temp{idx}_label")))
                    .map_or_else(|_| format!("temp{idx}"), |s| s.trim().to_string());
                let high_celsius = fs::read_to_string(path.join(format!("temp{idx}_max")))
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .filter(|&v| v > 0 && v < 150_000)
                    .map(|v| v as f32 / 1000.0);
                let crit_celsius = fs::read_to_string(path.join(format!("temp{idx}_crit")))
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .filter(|&v| v > 0 && v < 150_000)
                    .map(|v| v as f32 / 1000.0);
                let severity = classify_temp_severity(temp_celsius, high_celsius, crit_celsius);
                out.push(ThermalSensorReading {
                    chip: chip.clone(),
                    label,
                    temp_celsius,
                    high_celsius,
                    crit_celsius,
                    severity,
                });
            }
        }
    }

    if out.is_empty()
        && let Ok(entries) = fs::read_dir("/sys/class/thermal")
    {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("thermal_zone") {
                continue;
            }
            let path = entry.path();
            if let Ok(raw) = fs::read_to_string(path.join("temp"))
                && let Ok(milli) = raw.trim().parse::<i32>()
                && milli > 0
            {
                let temp_celsius = milli as f32 / 1000.0;
                let label = fs::read_to_string(path.join("type"))
                    .map_or_else(|_| name.clone(), |s| s.trim().to_string());
                let severity = classify_temp_severity(temp_celsius, Some(80.0), Some(95.0));
                out.push(ThermalSensorReading {
                    chip: name,
                    label,
                    temp_celsius,
                    high_celsius: Some(80.0),
                    crit_celsius: Some(95.0),
                    severity,
                });
            }
        }
    }

    out
}

fn classify_temp_severity(temp: f32, high: Option<f32>, crit: Option<f32>) -> HealthSeverity {
    let crit_limit = crit.unwrap_or(90.0);
    let warn_limit = high.unwrap_or(75.0);
    if temp >= crit_limit || temp >= 88.0 {
        HealthSeverity::Critical
    } else if temp >= warn_limit || temp >= 75.0 {
        HealthSeverity::Warning
    } else {
        HealthSeverity::Ok
    }
}

async fn collect_disk_health() -> Vec<DiskHealthReport> {
    let devices = discover_block_devices();
    let mut reports = Vec::new();
    for dev_name in devices {
        let dev_path = format!("/dev/{dev_name}");
        let mut smart_cmd = Command::new("smartctl");
        smart_cmd.args(["-j", "-a", &dev_path]);
        if let Ok(out) = crate::cmd::run_output_with_timeout(
            &mut smart_cmd,
            std::time::Duration::from_secs(5),
            "smartctl",
        )
        .await
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(report) = parse_smartctl_json(&dev_path, &text) {
                reports.push(report);
                continue;
            }
        }

        let mut nvme_cmd = Command::new("nvme");
        nvme_cmd.args(["smart-log", &dev_path, "-o", "json"]);
        if dev_name.starts_with("nvme")
            && let Ok(out) = crate::cmd::run_output_with_timeout(
                &mut nvme_cmd,
                std::time::Duration::from_secs(5),
                "nvme smart-log",
            )
            .await
            && out.status.success()
        {
            let model = fs::read_to_string(format!("/sys/block/{dev_name}/device/model"))
                .map_or_else(|_| "NVMe SSD".to_string(), |s| s.trim().to_string());
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(report) = parse_nvme_smart_log_json(&dev_path, &model, &text) {
                reports.push(report);
            }
        }
    }
    reports
}

fn discover_block_devices() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/block") else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("nvme") || name.starts_with("sd") || name.starts_with("vd") {
            out.push(name);
        }
    }
    out.sort();
    out
}

/// Parses `smartctl -j -a /dev/<disk>` JSON output for both `NVMe` and ATA/SATA drives.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn parse_smartctl_json(device: &str, raw: &str) -> Option<DiskHealthReport> {
    let root: Value = serde_json::from_str(raw).ok()?;
    let model = root
        .get("model_name")
        .and_then(Value::as_str)
        .or_else(|| root.get("scsi_model_name").and_then(Value::as_str))?
        .to_string();
    let serial = root
        .get("serial_number")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let protocol = root
        .pointer("/device/protocol")
        .and_then(Value::as_str)
        .unwrap_or("SMART")
        .to_string();
    let passed = root
        .pointer("/smart_status/passed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let temperature_celsius = root
        .pointer("/temperature/current")
        .and_then(Value::as_f64)
        .map(|v| v as f32);
    let power_on_hours = root
        .pointer("/power_on_time/hours")
        .and_then(Value::as_u64)
        .or_else(|| {
            root.pointer("/nvme_smart_health_information_log/power_on_hours")
                .and_then(Value::as_u64)
        });

    let mut percentage_used = root
        .pointer("/nvme_smart_health_information_log/percentage_used")
        .and_then(Value::as_u64)
        .map(|v| v.min(100) as u8);
    let available_spare_pct = root
        .pointer("/nvme_smart_health_information_log/available_spare")
        .and_then(Value::as_u64)
        .map(|v| v.min(100) as u8);
    let unsafe_shutdowns = root
        .pointer("/nvme_smart_health_information_log/unsafe_shutdowns")
        .and_then(Value::as_u64);
    let mut media_errors = root
        .pointer("/nvme_smart_health_information_log/media_errors")
        .and_then(Value::as_u64);

    // Fallback to ATA SMART attributes for SATA SSDs
    if let Some(table) = root
        .pointer("/ata_smart_attributes/table")
        .and_then(Value::as_array)
    {
        for attr in table {
            let name = attr.get("name").and_then(Value::as_str).unwrap_or("");
            let raw_val = attr
                .pointer("/raw/value")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let norm_val = attr.get("value").and_then(Value::as_u64).unwrap_or(100);
            if name == "Reallocated_Sector_Ct" && media_errors.is_none() {
                media_errors = Some(raw_val);
            } else if (name.contains("Wear_Leveling") || name.contains("Lifetime_Remain"))
                && percentage_used.is_none()
                && norm_val <= 100
            {
                percentage_used = Some((100 - norm_val) as u8);
            }
        }
    }

    let severity = classify_disk_severity(
        passed,
        temperature_celsius,
        percentage_used,
        available_spare_pct,
        media_errors,
    );

    Some(DiskHealthReport {
        device: device.to_string(),
        model,
        serial,
        protocol,
        passed,
        temperature_celsius,
        percentage_used,
        available_spare_pct,
        power_on_hours,
        unsafe_shutdowns,
        media_errors,
        severity,
    })
}

/// Parses `nvme smart-log /dev/nvmeXn1 -o json` output from `nvme-cli`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn parse_nvme_smart_log_json(device: &str, model: &str, raw: &str) -> Option<DiskHealthReport> {
    let root: Value = serde_json::from_str(raw).ok()?;
    let crit_warn = root.get("critical_warning")?.as_u64().unwrap_or(0);
    let passed = crit_warn == 0;
    let temp_k = root.get("temperature").and_then(Value::as_f64);
    let temperature_celsius = temp_k.map(|v| {
        if v > 200.0 {
            (v - 273.15) as f32
        } else {
            v as f32
        }
    });
    let percentage_used = root
        .get("percent_used")
        .or_else(|| root.get("percentage_used"))
        .and_then(Value::as_u64)
        .map(|v| v.min(100) as u8);
    let available_spare_pct = root
        .get("avail_spare")
        .or_else(|| root.get("available_spare"))
        .and_then(Value::as_u64)
        .map(|v| v.min(100) as u8);
    let power_on_hours = root.get("power_on_hours").and_then(Value::as_u64);
    let unsafe_shutdowns = root.get("unsafe_shutdowns").and_then(Value::as_u64);
    let media_errors = root.get("media_errors").and_then(Value::as_u64);

    let severity = classify_disk_severity(
        passed,
        temperature_celsius,
        percentage_used,
        available_spare_pct,
        media_errors,
    );

    Some(DiskHealthReport {
        device: device.to_string(),
        model: model.to_string(),
        serial: None,
        protocol: "NVMe".to_string(),
        passed,
        temperature_celsius,
        percentage_used,
        available_spare_pct,
        power_on_hours,
        unsafe_shutdowns,
        media_errors,
        severity,
    })
}

fn classify_disk_severity(
    passed: bool,
    temp_c: Option<f32>,
    pct_used: Option<u8>,
    avail_spare: Option<u8>,
    media_errors: Option<u64>,
) -> HealthSeverity {
    if !passed
        || media_errors.is_some_and(|e| e > 0)
        || pct_used.is_some_and(|u| u >= 90)
        || avail_spare.is_some_and(|s| s <= 10)
        || temp_c.is_some_and(|t| t >= 78.0)
    {
        HealthSeverity::Critical
    } else if pct_used.is_some_and(|u| u >= 80)
        || avail_spare.is_some_and(|s| s <= 20)
        || temp_c.is_some_and(|t| t >= 68.0)
    {
        HealthSeverity::Warning
    } else {
        HealthSeverity::Ok
    }
}

/// Evaluates all thermal sensors and disks, returning the overall highest severity and structured alerts.
pub fn evaluate_hardware_alerts(
    sensors: &[ThermalSensorReading],
    disks: &[DiskHealthReport],
) -> (HealthSeverity, Vec<HardwareAlert>) {
    let mut overall = HealthSeverity::Ok;
    let mut alerts = Vec::new();

    for s in sensors {
        if s.severity > overall {
            overall = s.severity;
        }
        match s.severity {
            HealthSeverity::Critical => {
                alerts.push(HardwareAlert {
                    severity: HealthSeverity::Critical,
                    source: format!("{} ({})", s.label, s.chip),
                    message: format!(
                        "温度严重超标: {:.1}°C 已超过临界阈值，请检查散热风道或风扇状态",
                        s.temp_celsius
                    ),
                });
            }
            HealthSeverity::Warning => {
                alerts.push(HardwareAlert {
                    severity: HealthSeverity::Warning,
                    source: format!("{} ({})", s.label, s.chip),
                    message: format!("温度偏高预警: 当前 {:.1}°C", s.temp_celsius),
                });
            }
            HealthSeverity::Ok => {}
        }
    }

    for d in disks {
        if d.severity > overall {
            overall = d.severity;
        }
        if !d.passed {
            alerts.push(HardwareAlert {
                severity: HealthSeverity::Critical,
                source: d.device.clone(),
                message: format!("磁盘 {} ({}) SMART 健康自检未通过", d.device, d.model),
            });
        }
        if let Some(errs) = d.media_errors
            && errs > 0
        {
            alerts.push(HardwareAlert {
                severity: HealthSeverity::Critical,
                source: d.device.clone(),
                message: format!("磁盘 {} 检测到 {errs} 个介质/坏块错误", d.device),
            });
        }
        if let Some(used) = d.percentage_used {
            if used >= 90 {
                alerts.push(HardwareAlert {
                    severity: HealthSeverity::Critical,
                    source: d.device.clone(),
                    message: format!("SSD 寿命磨损严重: 已消耗 {used}% 额定擦写寿命"),
                });
            } else if used >= 80 {
                alerts.push(HardwareAlert {
                    severity: HealthSeverity::Warning,
                    source: d.device.clone(),
                    message: format!("SSD 寿命磨损提醒: 已消耗 {used}% 额定擦写寿命"),
                });
            }
        }
        if let Some(temp) = d.temperature_celsius {
            if temp >= 78.0 {
                alerts.push(HardwareAlert {
                    severity: HealthSeverity::Critical,
                    source: d.device.clone(),
                    message: format!("SSD 盘体温度过高: {temp:.1}°C"),
                });
            } else if temp >= 68.0 {
                alerts.push(HardwareAlert {
                    severity: HealthSeverity::Warning,
                    source: d.device.clone(),
                    message: format!("SSD 盘体温度偏高: {temp:.1}°C"),
                });
            }
        }
    }

    (overall, alerts)
}

fn sample_dry_run_sensors() -> Vec<ThermalSensorReading> {
    vec![
        ThermalSensorReading {
            chip: "coretemp-isa-0000".to_string(),
            label: "Package id 0".to_string(),
            temp_celsius: 46.0,
            high_celsius: Some(80.0),
            crit_celsius: Some(100.0),
            severity: HealthSeverity::Ok,
        },
        ThermalSensorReading {
            chip: "coretemp-isa-0000".to_string(),
            label: "Core 0".to_string(),
            temp_celsius: 44.0,
            high_celsius: Some(80.0),
            crit_celsius: Some(100.0),
            severity: HealthSeverity::Ok,
        },
        ThermalSensorReading {
            chip: "nvme-pci-0100".to_string(),
            label: "Composite".to_string(),
            temp_celsius: 41.8,
            high_celsius: Some(74.8),
            crit_celsius: Some(84.8),
            severity: HealthSeverity::Ok,
        },
    ]
}

fn sample_dry_run_disks() -> Vec<DiskHealthReport> {
    vec![DiskHealthReport {
        device: "/dev/nvme0n1".to_string(),
        model: "CrabRouter Industrial NVMe 128GB".to_string(),
        serial: Some("CR-NVME-202611".to_string()),
        protocol: "NVMe".to_string(),
        passed: true,
        temperature_celsius: Some(42.0),
        percentage_used: Some(3),
        available_spare_pct: Some(100),
        power_on_hours: Some(1_840),
        unsafe_shutdowns: Some(2),
        media_errors: Some(0),
        severity: HealthSeverity::Ok,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lm_sensors_and_smartctl_nvme_and_triggers_alerts() {
        let sensors_json = r#"{
            "coretemp-isa-0000": {
                "Adapter": "ISA adapter",
                "Package id 0": {
                    "temp1_input": 82.5,
                    "temp1_max": 80.0,
                    "temp1_crit": 100.0
                }
            }
        }"#;
        let sensors = parse_sensors_json(sensors_json);
        assert_eq!(sensors.len(), 1);
        assert_eq!(sensors[0].severity, HealthSeverity::Warning);

        let smart_json = r#"{
            "model_name": "Samsung SSD 980 PRO",
            "serial_number": "S6B0NS0R123456",
            "device": { "protocol": "NVMe" },
            "smart_status": { "passed": true },
            "temperature": { "current": 45 },
            "nvme_smart_health_information_log": {
                "percentage_used": 92,
                "available_spare": 100,
                "power_on_hours": 12450,
                "unsafe_shutdowns": 4,
                "media_errors": 0
            }
        }"#;
        let disk = parse_smartctl_json("/dev/nvme0n1", smart_json).unwrap();
        assert_eq!(disk.model, "Samsung SSD 980 PRO");
        assert_eq!(disk.percentage_used, Some(92));
        assert_eq!(disk.severity, HealthSeverity::Critical);

        let (overall, alerts) = evaluate_hardware_alerts(&sensors, &[disk]);
        assert_eq!(overall, HealthSeverity::Critical);
        assert_eq!(alerts.len(), 2);
    }
}
