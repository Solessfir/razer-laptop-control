use crate::battery;
use crate::config;
use crate::dbus_mutter_idlemonitor;
use dbus::blocking::Connection;
use hidapi::HidApi;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;
use service::usb::RAZER_VENDOR_ID;
use service::SupportedDevice;
use std::{ffi::CString, io, thread, time};

#[derive(Serialize, Deserialize, Debug)]
pub struct RazerPacket {
    report: u8,
    status: u8,
    id: u8,
    remaining_packets: u16,
    protocol_type: u8,
    data_size: u8,
    command_class: u8,
    command_id: u8,
    #[serde(with = "BigArray")]
    args: [u8; 80],
    crc: u8,
    reserved: u8,
}

impl RazerPacket {
    const RAZER_CMD_NEW: u8 = 0x00;
    const RAZER_CMD_SUCCESSFUL: u8 = 0x02;

    fn new(command_class: u8, command_id: u8, data_size: u8) -> RazerPacket {
        return RazerPacket {
            report: 0x00,
            status: RazerPacket::RAZER_CMD_NEW,
            id: 0x1F,
            remaining_packets: 0x0000,
            protocol_type: 0x00,
            data_size,
            command_class,
            command_id,
            args: [0x00; 80],
            crc: 0x00,
            reserved: 0x00,
        };
    }

    fn calc_crc(&mut self) -> Vec<u8> {
        let mut res: u8 = 0x00;
        let buf: Vec<u8> = bincode::serialize(self).unwrap();
        for i in 2..88 {
            res ^= buf[i];
        }

        self.crc = res;
        return buf;
    }
}

pub struct DeviceManager {
    pub device: Option<RazerLaptop>,
    supported_devices: Vec<SupportedDevice>,
    pub config: Option<config::Configuration>,
    pub idle_id: u32,
    pub active_id: u32,
    add_active: bool,
    pub change_idle: bool,
}

impl DeviceManager {
    pub fn new() -> DeviceManager {
        return DeviceManager {
            device: None,
            supported_devices: vec![],
            config: None,
            idle_id: 0,
            active_id: 0,
            add_active: false,
            change_idle: false,
        };
    }

    pub fn add_idle_watch(
        &mut self,
        proxy_idle: &dyn dbus_mutter_idlemonitor::OrgGnomeMutterIdleMonitor,
    ) {
        if self.change_idle {
            let mut timeout: u64 = 0;
            let mut state: usize = 0;
            if let Some(laptop) = self.get_device() {
                state = laptop.get_ac_state();
            }
            if let Some(config) = self.get_config() {
                timeout = config.power[state].idle as u64 * 60 * 1000; // idle is in minutes timeout is in miliseconds
            }
            if timeout != 0 {
                if self.idle_id != 0 {
                    self.remove_watch(proxy_idle);
                }
                if let Ok(id) = proxy_idle.add_idle_watch(timeout) {
                    println!("idle handler {:?}", id);
                    self.idle_id = id;
                }
            } else {
                if self.idle_id != 0 {
                    self.remove_watch(proxy_idle);
                }
            }
            self.change_idle = false;
        }
    }

    pub fn set_sync(&mut self, sync: bool) -> bool {
        let mut ac: usize = 0;
        if let Some(laptop) = self.get_device() {
            ac = laptop.ac_state as usize;
        }
        let other = (ac + 1) & 0x01;
        if let Some(config) = self.get_config() {
            config.sync = sync;
            config.power[other].brightness = config.power[ac].brightness;
            config.power[other].logo_state = config.power[ac].logo_state;
            config.power[other].screensaver = config.power[ac].screensaver;
            config.power[other].idle = config.power[ac].idle;
            if let Err(e) = config.write_to_file() {
                eprintln!("Error write config {:?}", e);
            }
        }

        return true;
    }

    pub fn get_sync(&mut self) -> bool {
        if let Some(config) = self.get_config() {
            return config.sync;
        }

        return false;
    }

    pub fn set_light_control(&mut self, enabled: bool) -> bool {
        if let Some(config) = self.get_config() {
            if config.enable_light_control != enabled {
                config.enable_light_control = enabled;
                if let Err(e) = config.write_to_file() {
                    eprintln!("Error write config {:?}", e);
                }
            }
        }

        return true;
    }

    pub fn get_light_control(&mut self) -> bool {
        if let Some(config) = self.get_config() {
            return config.enable_light_control;
        }

        return false;
    }

    fn remove_watch(
        &mut self,
        proxy_idle: &dyn dbus_mutter_idlemonitor::OrgGnomeMutterIdleMonitor,
    ) {
        if let Ok(_) = proxy_idle.remove_watch(self.idle_id) {
            println!("remove idle handler");
        }
    }

    pub fn add_active_watch(
        &mut self,
        proxy_idle: &dyn dbus_mutter_idlemonitor::OrgGnomeMutterIdleMonitor,
    ) {
        if self.add_active {
            if let Ok(id) = proxy_idle.add_user_active_watch() {
                println!("active handler {:?}", id);
                self.active_id = id;
            }
        }
    }

    pub fn read_laptops_file() -> io::Result<DeviceManager> {
        let device_data = service::get_device_data();
        let mut res: DeviceManager = DeviceManager::new();
        res.supported_devices = serde_json::from_str(&device_data)?;
        println!("suported devices found: {:?}", res.supported_devices.len());
        match config::Configuration::read_from_config() {
            Ok(c) => res.config = Some(c),
            Err(_) => res.config = Some(config::Configuration::new()),
        }

        Ok(res)
    }

    fn get_ac_config(&mut self, ac: usize) -> Option<config::PowerConfig> {
        if let Some(c) = self.get_config() {
            return Some(c.power[ac]);
        }

        return None;
    }

    pub fn light_off(&mut self) {
        if self.idle_id != 0 {
            self.add_active = true;
        }
        if let Some(laptop) = self.get_device() {
            laptop.set_screensaver(true);
            laptop.set_brightness(0);
            laptop.set_logo_led_state(0);
        }
    }

    pub fn restore_light(&mut self) {
        self.add_active = false;
        let mut brightness = 0;
        let mut logo_state = 0;
        let mut ac: usize = 0;
        if let Some(laptop) = self.get_device() {
            ac = laptop.get_ac_state();
        }
        if let Some(config) = self.get_ac_config(ac) {
            brightness = config.brightness;
            logo_state = config.logo_state;
        }
        if let Some(laptop) = self.get_device() {
            laptop.set_screensaver(false);
            laptop.set_brightness(brightness);
            laptop.set_logo_led_state(logo_state);
        }
    }

    pub fn restore_standard_effect(&mut self) {
        if !self.get_light_control() {
            return;
        }

        let mut effect = 0;
        let mut params: Vec<u8> = vec![];
        if let Some(config) = self.get_config() {
            effect = config.standard_effect;
            params = config.standard_effect_params.clone();
        }
        if let Some(laptop) = self.get_device() {
            laptop.set_standard_effect(effect, params);
        }
    }

    pub fn change_idle(&mut self, ac: usize, timeout: u32) -> bool {
        if let Some(config) = self.get_config() {
            if config.power[ac].idle != timeout {
                config.power[ac].idle = timeout;
                if config.sync {
                    let other = (ac + 1) & 0x01;
                    config.power[other].idle = timeout;
                }
                if let Err(e) = config.write_to_file() {
                    eprintln!("Error write config {:?}", e);
                }
                self.change_idle = true;
            }
        }

        return true;
    }

    pub fn set_power_mode(&mut self, ac: usize, pwr: u8, cpu: u8, gpu: u8) -> bool {
        let mut res: bool = false;
        if let Some(config) = self.get_config() {
            config.power[ac].power_mode = pwr;
            config.power[ac].cpu_boost = cpu;
            config.power[ac].gpu_boost = gpu;
            if let Err(e) = config.write_to_file() {
                eprintln!("Error write config {:?}", e);
            }
        }
        if let Some(laptop) = self.get_device() {
            let state = laptop.get_ac_state();
            if state != ac {
                res = true;
            } else {
                res = laptop.set_power_mode(pwr, cpu, gpu);
            }
        }

        return res;
    }

    pub fn set_standard_effect(&mut self, effect_id: u8, params: Vec<u8>) -> bool {
        if let Some(config) = self.get_config() {
            config.standard_effect = effect_id;
            config.standard_effect_params = params.clone();
            if let Err(e) = config.write_to_file() {
                eprintln!("Error write config {:?}", e);
            }
        }
        if let Some(laptop) = self.get_device() {
            laptop.set_standard_effect(effect_id, params);
        }

        return true;
    }

    pub fn set_fan_rpm(&mut self, ac: usize, rpm: i32) -> bool {
        let mut res: bool = false;
        if let Some(config) = self.get_config() {
            config.power[ac].fan_rpm = rpm;
            if let Err(e) = config.write_to_file() {
                eprintln!("Error write config {:?}", e);
            }
        }

        if let Some(laptop) = self.get_device() {
            let state = laptop.get_ac_state();
            if state != ac {
                res = true;
            } else {
                res = laptop.set_fan_rpm(rpm as u16);
            }
        }

        return res;
    }

    pub fn set_logo_led_state(&mut self, ac: usize, logo_state: u8) -> bool {
        let mut res: bool = false;
        if let Some(config) = self.get_config() {
            config.power[ac].logo_state = logo_state;
            if config.sync {
                let other = (ac + 1) & 0x01;
                config.power[other].logo_state = logo_state;
            }
            if let Err(e) = config.write_to_file() {
                eprintln!("Error write config {:?}", e);
            }
        }

        if let Some(laptop) = self.get_device() {
            let state = laptop.get_ac_state();

            if state != ac {
                res = true;
            } else {
                res = laptop.set_logo_led_state(logo_state);
            }
        }

        return res;
    }

    pub fn get_logo_led_state(&mut self, ac: usize) -> u8 {
        if let Some(config) = self.get_ac_config(ac) {
            return config.logo_state;
        }

        return 0;
    }

    pub fn set_brightness(&mut self, ac: usize, brightness: u8) -> bool {
        let mut res: bool = false;
        let _val = brightness as u16 * 255 / 100;
        if let Some(config) = self.get_config() {
            config.power[ac].brightness = _val as u8;
            if config.sync {
                let other = (ac + 1) & 0x01;
                config.power[other].brightness = _val as u8;
            }
            if let Err(e) = config.write_to_file() {
                eprintln!("Error write config {:?}", e);
            }
        }

        if let Some(laptop) = self.get_device() {
            let state = laptop.get_ac_state();
            if state != ac {
                res = true;
            } else {
                res = laptop.set_brightness(_val as u8);
            }
        }

        return res;
    }

    pub fn get_brightness(&mut self, ac: usize) -> u8 {
        if let Some(laptop) = self.get_device() {
            if laptop.ac_state as usize == ac {
                let val = laptop.get_brightness() as u32;
                let mut perc = val * 100 * 100 / 255;
                perc += 50;
                perc /= 100;
                return perc as u8;
            }
        }

        if let Some(config) = self.get_ac_config(ac) {
            let val = config.brightness as u32;
            let mut perc = val * 100 * 100 / 255;
            perc += 50;
            perc /= 100;
            return perc as u8;
        }

        return 0;
    }

    pub fn get_fan_rpm(&mut self, ac: usize) -> i32 {
        if let Some(laptop) = self.get_device() {
            if laptop.ac_state as usize == ac {
                return laptop.get_fan_rpm() as i32;
            }
        }

        if let Some(config) = self.get_ac_config(ac) {
            return config.fan_rpm;
        }

        return 0;
    }

    pub fn get_power_mode(&mut self, ac: usize) -> u8 {
        if let Some(config) = self.get_ac_config(ac) {
            return config.power_mode;
        }

        return 0;
    }

    pub fn get_cpu_boost(&mut self, ac: usize) -> u8 {
        if let Some(config) = self.get_ac_config(ac) {
            return config.cpu_boost;
        }

        return 0;
    }

    pub fn get_gpu_boost(&mut self, ac: usize) -> u8 {
        if let Some(config) = self.get_ac_config(ac) {
            return config.gpu_boost;
        }

        return 0;
    }

    pub fn set_ac_state(&mut self, ac: bool) {
        if let Some(laptop) = self.get_device() {
            laptop.set_ac_state(ac);
        }
        self.change_idle = true;
        let config: Option<config::PowerConfig> = self.get_ac_config(ac as usize);
        if let Some(config) = config {
            if let Some(laptop) = self.get_device() {
                laptop.set_config(config);
            }
        }
    }

    pub fn set_ac_state_get(&mut self) {
        let dbus_system = Connection::new_system().expect("failed to connect to D-Bus system bus");
        let proxy_ac = dbus_system.with_proxy(
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower/devices/line_power_AC0",
            time::Duration::from_millis(5000),
        );
        use battery::OrgFreedesktopUPowerDevice;
        if let Ok(online) = proxy_ac.online() {
            if let Some(laptop) = self.get_device() {
                laptop.set_ac_state(online);
            }
            self.change_idle = true;
            let config: Option<config::PowerConfig> = self.get_ac_config(online as usize);
            if let Some(config) = config {
                if let Some(laptop) = self.get_device() {
                    laptop.set_config(config);
                }
            }
        }
    }

    pub fn get_device(&mut self) -> Option<&mut RazerLaptop> {
        return self.device.as_mut();
    }

    pub fn set_bho_handler(&mut self, is_on: bool, threshold: u8) -> bool {
        return self
            .get_device()
            .map_or(false, |laptop| laptop.set_bho(is_on, threshold));
    }

    pub fn get_bho_handler(&mut self) -> Option<(bool, u8)> {
        return self
            .get_device()
            .and_then(|laptop| laptop.get_bho().map(|result| byte_to_bho(result)));
    }

    fn get_config(&mut self) -> Option<&mut config::Configuration> {
        return self.config.as_mut();
    }

    pub fn find_supported_device(&mut self, vid: u16, pid: u16) -> Option<&SupportedDevice> {
        for device in &self.supported_devices {
            // Unwrap: we control the strings and know they are are valid
            let svid = u16::from_str_radix(&device.vid, 16).unwrap();
            let spid = u16::from_str_radix(&device.pid, 16).unwrap();

            if svid == vid && spid == pid {
                return Some(device);
            }
        }

        None
    }

    pub fn discover_devices(&mut self) {
        match HidApi::new() {
            Ok(api) => {
                // First pass: find the main control interface
                let mut found: Option<(
                    CString,
                    u16,
                    u16,
                    String,
                    Vec<String>,
                    Vec<u16>,
                    Option<i32>,
                )> = None;

                for device in api
                    .device_list()
                    .filter(|d| d.vendor_id() == RAZER_VENDOR_ID)
                {
                    let result =
                        self.find_supported_device(device.vendor_id(), device.product_id());
                    if let Some(supported) = result {
                        let required_interface = supported.interface.unwrap_or(0);
                        if device.interface_number() == required_interface {
                            found = Some((
                                device.path().to_owned(),
                                device.vendor_id(),
                                device.product_id(),
                                supported.name.clone(),
                                supported.features.clone(),
                                supported.fan.clone(),
                                supported.kbd_interface,
                            ));
                            break;
                        }
                    }
                }

                if let Some((path, vid, pid, name, features, fan, kbd_interface)) = found {
                    // Find kbd interface path if specified
                    let kbd_path = kbd_interface.and_then(|iface| {
                        api.device_list()
                            .find(|d| {
                                d.vendor_id() == vid
                                    && d.product_id() == pid
                                    && d.interface_number() == iface
                            })
                            .map(|d| d.path().to_owned())
                    });

                    match api.open_path(&path) {
                        Ok(dev) => {
                            let kbd_device = kbd_path.and_then(|kp| api.open_path(&kp).ok());
                            self.device =
                                Some(RazerLaptop::new(name, features, fan, dev, kbd_device));
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                        }
                    }
                } else {
                    eprintln!("no supported device found");
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
    }
}

pub struct RazerLaptop {
    name: String,
    features: Vec<String>,
    fan: Vec<u16>,
    device: hidapi::HidDevice,
    kbd_device: Option<hidapi::HidDevice>,
    power: u8,    // need for fan
    fan_rpm: u8,  // need for power
    ac_state: u8, // index config array
    screensaver: bool,
    brightness: u8, // re-applied after mode switch to counteract firmware reset
    custom_frame_active: bool, // true once keyboard is in custom frame mode
}

impl RazerLaptop {
    const NOSTORE: u8 = 0x00;
    const VARSTORE: u8 = 0x01;
    const LOGO_LED: u8 = 0x04;
    const BACKLIGHT_LED: u8 = 0x05;
    pub const OFF: u8 = 0x00;
    pub const WAVE: u8 = 0x01;
    pub const REACTIVE: u8 = 0x02;
    pub const BREATHING: u8 = 0x03;
    pub const SPECTRUM: u8 = 0x04;
    pub const CUSTOMFRAME: u8 = 0x05;
    pub const STATIC: u8 = 0x06;
    pub const STARLIGHT: u8 = 0x19;

    pub fn new(
        name: String,
        features: Vec<String>,
        fan: Vec<u16>,
        device: hidapi::HidDevice,
        kbd_device: Option<hidapi::HidDevice>,
    ) -> RazerLaptop {
        return RazerLaptop {
            name,
            features,
            fan,
            device,
            kbd_device,
            power: 0,
            fan_rpm: 0,
            ac_state: 0,
            screensaver: false,
            brightness: 0,
            custom_frame_active: false,
        };
    }

    pub fn set_screensaver(&mut self, active: bool) {
        self.screensaver = active;
    }

    pub fn set_config(&mut self, config: config::PowerConfig) -> bool {
        let mut ret: bool = false;

        if !self.screensaver {
            ret |= self.set_brightness(config.brightness);
            ret |= self.set_logo_led_state(config.logo_state);
        } else {
            ret |= self.set_brightness(0);
            ret |= self.set_logo_led_state(0);
        }
        ret |= self.set_power_mode(config.power_mode, config.cpu_boost, config.gpu_boost);
        ret |= self.set_fan_rpm(config.fan_rpm as u16);

        return ret;
    }

    pub fn set_ac_state(&mut self, online: bool) -> usize {
        if online {
            self.ac_state = 1;
        } else {
            self.ac_state = 0;
        }

        return self.ac_state as usize;
    }

    pub fn get_ac_state(&mut self) -> usize {
        return self.ac_state as usize;
    }

    pub fn get_name(&self) -> String {
        return self.name.clone();
    }

    pub fn have_feature(&mut self, fch: String) -> bool {
        return self.features.contains(&fch);
    }

    fn clamp_fan(&mut self, rpm: u16) -> u8 {
        if rpm > self.fan[1] {
            return (self.fan[1] / 100) as u8;
        }
        if rpm < self.fan[0] {
            return (self.fan[0] / 100) as u8;
        }

        return (rpm / 100) as u8;
    }

    fn clamp_u8(&mut self, value: u8, min: u8, max: u8) -> u8 {
        if value > max {
            return max;
        }
        if value < min {
            return min;
        }

        return value;
    }

    pub fn set_standard_effect(&mut self, effect_id: u8, params: Vec<u8>) -> bool {
        self.custom_frame_active = false;
        let mut report: RazerPacket = RazerPacket::new(0x03, 0x0a, 80);
        report.args[0] = effect_id; // effect id
        if !params.is_empty() {
            for idx in 0..params.len() {
                report.args[idx + 1] = params[idx];
            }
        }
        if let Some(_) = self.send_report(report) {
            return true;
        }

        return false;
    }

    pub fn set_custom_frame_data(&mut self, row: u8, data: Vec<u8>) {
        if data.len() == 45 {
            let mut report: RazerPacket = RazerPacket::new(0x03, 0x0b, 0x34);
            report.args[0] = 0xff;
            report.args[1] = row;
            report.args[2] = 0x00; // start col
            report.args[3] = 0x0f; // end col
            for idx in 0..data.len() {
                report.args[idx + 7] = data[idx];
            }
            self.send_report(report);
        }
    }

    pub fn set_custom_frame(&mut self) -> bool {
        let mut report: RazerPacket = RazerPacket::new(0x03, 0x0a, 0x02);
        report.args[0] = RazerLaptop::CUSTOMFRAME; // effect id
        report.args[1] = RazerLaptop::NOSTORE;
        let ok = self.send_report(report).is_some();
        // Firmware resets brightness when transitioning into custom frame mode.
        // Re-apply it once on the first activation; subsequent frames don't need this.
        if ok && !self.custom_frame_active && self.brightness > 0 {
            self.custom_frame_active = true;
            let brightness = self.brightness;
            self.set_brightness(brightness);
        }
        return ok;
    }

    fn map_power_mode(mode: u8) -> u8 {
        match mode {
            0 => 0x00, // Balanced
            1 => 0x01, // Gaming
            2 => 0x02, // Creator
            3 => 0x05, // Silent
            4 => 0x04, // Custom
            _ => 0x00,
        }
    }

    fn send_preamble(&mut self, mode: u8) -> bool {
        let report1: RazerPacket = RazerPacket::new(0x07, 0x8f, 0x01);
        let ok1 = self.send_report(report1).is_some();
        let mut report2: RazerPacket = RazerPacket::new(0x07, 0x0f, 0x01);
        report2.args[0] = mode;
        let ok2 = self.send_report(report2).is_some();
        ok1 && ok2
    }

    fn set_power(&mut self, zone: u8) -> bool {
        let mut report: RazerPacket = RazerPacket::new(0x0d, 0x02, 0x04);
        report.args[0] = 0x01;
        report.args[1] = zone;
        report.args[2] = self.power;
        match self.fan_rpm {
            0 => report.args[3] = 0x00,
            _ => report.args[3] = 0x01,
        }
        if let Some(_) = self.send_report(report) {
            return true;
        }

        return false;
    }

    fn set_tdp_tier(&mut self, zone: u8, tier: u8) -> bool {
        let mut report: RazerPacket = RazerPacket::new(0x0d, 0x07, 0x03);
        report.args[0] = 0x01;
        report.args[1] = zone;
        report.args[2] = tier;
        if let Some(_) = self.send_report(report) {
            return true;
        }
        return false;
    }

    fn set_cpu_boost(&mut self, mut boost: u8) -> bool {
        let mut report: RazerPacket = RazerPacket::new(0x0d, 0x07, 0x03);
        if boost == 3 && !self.have_feature("boost".to_string()) {
            boost = 2;
        }
        report.args[0] = 0x00;
        report.args[1] = 0x01;
        report.args[2] = boost;
        if let Some(_) = self.send_report(report) {
            return true;
        }

        return false;
    }

    fn set_gpu_boost(&mut self, boost: u8) -> bool {
        let mut report: RazerPacket = RazerPacket::new(0x0d, 0x07, 0x03);
        report.args[0] = 0x00;
        report.args[1] = 0x02;
        report.args[2] = boost;
        if let Some(_) = self.send_report(report) {
            return true;
        }
        return false;
    }

    pub fn set_power_mode(&mut self, mode: u8, cpu_boost: u8, gpu_boost: u8) -> bool {
        let hw_mode = Self::map_power_mode(mode);
        if mode <= 3 {
            self.power = hw_mode;
            self.fan_rpm = 0; // revert to automatic fan control
            if !self.send_preamble(0x0c) {
                eprintln!("Warning: power mode preamble timed out, proceeding anyway");
            }
            self.set_power(0x01);
            self.set_power(0x02);
            self.set_power(0x03);
            self.set_power(0x04);
        } else if mode == 4 {
            self.power = hw_mode;
            self.fan_rpm = 0;
            if !self.send_preamble(0x0c) {
                eprintln!("Warning: power mode preamble timed out, proceeding anyway");
            }
            self.set_power(0x01);
            self.set_power(0x02);
            self.set_power(0x03);
            self.set_power(0x04);
            self.set_tdp_tier(0x01, 0x03); // CPU TDP tier
            self.set_tdp_tier(0x02, 0x02); // GPU TDP tier
            self.set_cpu_boost(cpu_boost);
            self.set_gpu_boost(gpu_boost);
        }

        return true;
    }

    fn set_rpm(&mut self, zone: u8) -> bool {
        let mut report: RazerPacket = RazerPacket::new(0x0d, 0x01, 0x03);
        report.args[0] = 0x01;
        report.args[1] = zone;
        report.args[2] = self.fan_rpm;
        if let Some(_) = self.send_report(report) {
            return true;
        }

        return false;
    }

    pub fn set_fan_rpm(&mut self, value: u16) -> bool {
        if self.power != 4 {
            if value == 0 {
                self.fan_rpm = 0;
                if !self.send_preamble(0x0c) {
                    eprintln!("Warning: fan preamble timed out, proceeding anyway");
                }
                self.set_power(0x01);
                self.set_power(0x02);
                self.set_power(0x03);
                self.set_power(0x04);
            } else {
                self.fan_rpm = self.clamp_fan(value);
                if !self.send_preamble(0x08) {
                    eprintln!("Warning: fan preamble timed out, proceeding anyway");
                }
                self.set_power(0x01);
                self.set_power(0x02);
                self.set_power(0x03);
                self.set_power(0x04);
                self.set_rpm(0x01);
                self.set_rpm(0x02);
                self.set_rpm(0x03);
                self.set_rpm(0x04);
            }
        }

        return true;
    }

    pub fn get_fan_rpm(&mut self) -> u16 {
        let res: u16 = self.fan_rpm as u16;
        return res * 100;
    }

    pub fn set_logo_led_state(&mut self, mode: u8) -> bool {
        if mode > 0 {
            let mut report: RazerPacket = RazerPacket::new(0x03, 0x02, 0x03);
            report.args[0] = RazerLaptop::VARSTORE;
            report.args[1] = RazerLaptop::LOGO_LED;
            if mode == 1 {
                report.args[2] = 0x00;
            } else if mode == 2 {
                report.args[2] = 0x02;
            }
            self.send_report(report);
        }

        let mut report: RazerPacket = RazerPacket::new(0x03, 0x00, 0x03);
        report.args[0] = RazerLaptop::VARSTORE;
        report.args[1] = RazerLaptop::LOGO_LED;
        report.args[2] = self.clamp_u8(mode, 0x00, 0x01);
        if let Some(_) = self.send_report(report) {
            return true;
        }

        return false;
    }

    pub fn set_brightness(&mut self, brightness: u8) -> bool {
        self.brightness = brightness;
        let mut report: RazerPacket = RazerPacket::new(0x03, 0x03, 0x03);
        report.args[0] = RazerLaptop::VARSTORE;
        report.args[1] = RazerLaptop::BACKLIGHT_LED;
        report.args[2] = brightness;
        if let Some(_) = self.send_report(report) {
            return true;
        }

        return false;
    }

    pub fn get_brightness(&mut self) -> u8 {
        let mut report: RazerPacket = RazerPacket::new(0x03, 0x83, 0x03);
        report.args[0] = RazerLaptop::VARSTORE;
        report.args[1] = RazerLaptop::BACKLIGHT_LED;
        report.args[2] = 0x00;
        if let Some(response) = self.send_report(report) {
            return response.args[2];
        }
        return 0;
    }

    pub fn get_bho(&mut self) -> Option<u8> {
        if !self.have_feature("bho".to_string()) {
            return None;
        }

        let mut report: RazerPacket = RazerPacket::new(0x07, 0x92, 0x01);
        report.args[0] = 0x00;

        return self.send_report(report).map(|resp| resp.args[0]);
    }

    pub fn set_bho(&mut self, is_on: bool, threshold: u8) -> bool {
        if !self.have_feature("bho".to_string()) {
            return false;
        }

        let mut report = RazerPacket::new(0x07, 0x12, 0x01);
        report.args[0] = bho_to_byte(is_on, threshold);

        return self.send_report(report).is_some();
    }

    fn send_report(&mut self, mut report: RazerPacket) -> Option<RazerPacket> {
        let mut temp_buf: [u8; 91] = [0x00; 91];
        let use_kbd = report.command_class == 0x03 && self.kbd_device.is_some();
        for _ in 0..3 {
            let send_result = if use_kbd {
                self.kbd_device
                    .as_mut()
                    .unwrap()
                    .send_feature_report(report.calc_crc().as_slice())
            } else {
                self.device
                    .send_feature_report(report.calc_crc().as_slice())
            };
            match send_result {
                Ok(_) => {
                    thread::sleep(time::Duration::from_micros(1000));
                    let recv_result = if use_kbd {
                        self.kbd_device
                            .as_mut()
                            .unwrap()
                            .get_feature_report(&mut temp_buf)
                    } else {
                        self.device.get_feature_report(&mut temp_buf)
                    };
                    match recv_result {
                        Ok(size) => {
                            if size == 91 {
                                match bincode::deserialize::<RazerPacket>(&temp_buf) {
                                    Ok(response) => {
                                        // when request bho status the response command id is different from the request command id...
                                        if response.command_id == 0x92 {
                                            return Some(response);
                                        }

                                        if response.remaining_packets != report.remaining_packets
                                            || response.command_class != report.command_class
                                            || response.command_id != report.command_id
                                        {
                                            eprintln!("Response doesn't match request: cls={:02x}/{:02x} id={:02x}/{:02x} status={:02x}",
                                                report.command_class, response.command_class,
                                                report.command_id, response.command_id,
                                                response.status);
                                        } else if response.status
                                            == RazerPacket::RAZER_CMD_SUCCESSFUL
                                        {
                                            return Some(response);
                                        } else {
                                            eprintln!("Command cls={:02x} id={:02x} failed with status={:02x}",
                                                report.command_class, report.command_id, response.status);
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Error: {}", e);
                                    }
                                }
                            } else {
                                eprintln!("Invalid report length: {:?}", size);
                            }
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                }
            };
        }

        thread::sleep(time::Duration::from_micros(8000));
        return None;
    }
}

// top bit flags whether battery health optimization is on or off
// bottom bits are the actual threshold that it is set to
fn byte_to_bho(u: u8) -> (bool, u8) {
    return (u & (1 << 7) != 0, (u & 0b0111_1111));
}

fn bho_to_byte(is_on: bool, threshold: u8) -> u8 {
    if is_on {
        return threshold | 0b1000_0000;
    }
    return threshold;
}
