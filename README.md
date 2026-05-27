# Razer Laptop Control

This is an experimental application designed to control certain features
available on Razer laptops. It is not officially supported by Razer, nor does
it have dedicated maintainers for each device. Use it at your own risk.

Support for devices is community-driven. Each supported model has been at least
partially tested by the contributor who added it, but functionality may be
incomplete or vary between devices.

![](Screenshoot.png)

## Features:

- Light control (you can opt out if you are using OpenRazer)
- Battery charging limits
- Power profiles
- Fan control
- Logo control
- Basic GUI
- CLI

## Supported Models

| Model                         | VID  | PID  | Features                  |
| ----------------------------- | ---- | ---- | ------------------------- |
| Blade 15 2016                 | 1532 | 0224 | logo                      |
| Blade 2018 15 advanced        | 1532 | 0233 | logo                      |
| Blade 2018 15 base            | 1532 | 023B | logo                      |
| Blade 2018 15 Mercury edition | 1532 | 0240 | logo                      |
| Blade 2019 15 base            | 1532 | 0246 | logo                      |
| Blade 2019 15 advanced        | 1532 | 023A | logo, creator_mode        |
| Blade 2019 15 Mercury edition | 1532 | 0245 | logo, creator_mode        |
| Blade 2020 15 base            | 1532 | 0255 | logo                      |
| Blade 2020 15 advanced        | 1532 | 0253 | logo, creator_mode, boost |
| Blade 2017 stealth            | 1532 | 022D | logo                      |
| late Blade 2017 stealth       | 1532 | 0232 | logo                      |
| Blade 2019 stealth            | 1532 | 0239 | logo                      |
| Blade 2019 stealth (With GTX) | 1532 | 024A | logo                      |
| Blade 2020 stealth            | 1532 | 0252 | logo                      |
| Blade 2020 pro                | 1532 | 0256 | logo                      |
| Blade 2019 pro                | 1532 | 0234 | logo                      |
| Blade 2018 pro FHD            | 1532 | 022F | logo                      |
| Blade 2017 pro                | 1532 | 0225 | logo                      |
| Blade 2017 pro                | 1532 | 0210 | logo                      |
| Blade QHD                     | 1532 | 020F | logo                      |
| Book 2020 13                  | 1532 | 026A |                           |
| Blade 2021 15 base            | 1532 | 026F | logo                      |
| Blade 14 2021                 | 1532 | 0270 | logo                      |
| Blade 2021 15 advanced        | 1532 | 0276 | logo, boost               |
| Blade late 2021 15 advanced   | 1532 | 026D | logo, boost               |
| Blade late 2021 15 base       | 1532 | 027A | logo                      |
| Blade early 2022 15 advanced  | 1532 | 028A | logo, boost, bho          |
| Blade 2022 17                 | 1532 | 028B | logo, boost               |
| Blade 2022 14                 | 1532 | 028C | logo, boost, bho          |
| Blade late 2020 stealth       | 1532 | 0259 | logo                      |
| Blade late 16                 | 1532 | 029F | logo, boost, bho          |
| Blade 2023 14                 | 1532 | 029D | logo, boost, bho          |
| Blade Early 2021 17 pro       | 1532 | 026E | logo, boost               |
| Blade 2024 14                 | 1532 | 02b6 | logo, boost, bho          |
| Blade Mid 2021 17 pro         | 1532 | 0279 | logo, boost               |
| Razer Blade 18 2023           | 1532 | 02a0 | logo, boost, bho          |
| Razer Blade 16 2024           | 1532 | 02b7 | logo, boost, bho          |
| Razer Blade 18 2024           | 1532 | 02b8 | logo, boost, bho          |

### Adding a new model

If your model is not currently supported, please open a new issue to request
support. Include the output of `razer-cli device-info`, and we’ll do our best
to assist you.

Be prepared to provide additional details about your device (such as fan limits
and available features) and to help test any proposed changes.

## Supported Init Systems

In order to run as a service this application runs as a `systemd` daemon. There
is also code to run in `openrc`, but it is currently unmaintained.

## Supported Installation Methods

We currently support installing the application via installation script and via
NixOS Flake. The flake has no known maintainers but we try to keep it working.

### Installing via prebuilt binary

> [!WARNING]
> Tested on Arch Linux only.

Using `curl`:

```sh
curl -sSL https://raw.githubusercontent.com/Solessfir/razer-laptop-control/main/install-bin.sh | bash -s install
```

Using `wget`:

```sh
wget -qO- https://raw.githubusercontent.com/Solessfir/razer-laptop-control/main/install-bin.sh | bash -s install
```

### Installing via script

Before installing, the following dependencies are needed:

- Rust available on your terminal
- The following packages or their equivalent:
    - `libdbus-1-dev libusb-dev libhidapi-dev libhidapi-hidraw0 pkg-config libudev-dev libgtk-3-dev`

With those installed, run `./install.sh install` as a normal user. Then reboot.

### Installing via Nixos Flake

1. Add this flake to your inputs using

```
inputs.razerdaemon.url = "github:JosuGZ/razer-laptop-control";
```

2. Import the razerdaemon module where your inputs are in scope

```
imports = [
    inputs.razerdaemon.nixosModules.default
];
```

3. Enable the exposed nixos option using

```
services.razer-laptop-control.enable = true;
```

### Uninstalling

> [!WARNING]
> Tested on Arch Linux only. Removes binaries, services, udev rules,
> desktop entry and icon installed by the binary installer.

Using `curl`:

```sh
curl -sSL https://raw.githubusercontent.com/Solessfir/razer-laptop-control/main/install-bin.sh | bash -s uninstall
```

Using `wget`:

```sh
wget -qO- https://raw.githubusercontent.com/Solessfir/razer-laptop-control/main/install-bin.sh | bash -s uninstall
```

## CLI Usage

Top-level subcommands:

| Subcommand                          | Purpose                                  |
| ----------------------------------- | ---------------------------------------- |
| `read <attr> <ac\|bat>`             | Read current value of an attribute       |
| `write <attr> <ac\|bat> <args>`     | Change and persist an attribute          |
| `standard-effect <effect> <args>`   | Apply firmware-side keyboard effect      |
| `effect <effect> <args>`            | Apply daemon-side animated effect        |
| `device-info`                       | List detected Razer devices              |

### Attributes (for `read` / `write`)

| Attribute       | Args                       | Description                                              |
| --------------- | -------------------------- | -------------------------------------------------------- |
| `fan`           | `<rpm>`                    | `0` = Auto, other = manual RPM                           |
| `power`         | `<mode> [cpu] [gpu]`       | See Power Modes below                                    |
| `brightness`    | `<0–100>`                  | Keyboard brightness percent                              |
| `logo`          | `<0\|1\|2>`                | 0=Off, 1=On, 2=Breathing                                 |
| `sync`          | `on\|off`                  | Sync AC and battery light profiles                       |
| `bho`           | `on\|off [threshold]`      | Battery Health Optimization, optional charge cap %       |
| `light-control` | `on\|off`                  | Enable/disable daemon lighting control                   |

> [!NOTE]
> Brightness changed via `Fn` keys is not saved by the daemon. Use
> `razer-cli write brightness` or the GUI to set a persistent value.

### Power Modes

| ID | Mode     | Notes                                          |
| -- | -------- | ---------------------------------------------- |
| 0  | Balanced |                                                |
| 1  | Gaming   |                                                |
| 2  | Creator  |                                                |
| 3  | Silent   |                                                |
| 4  | Custom   | Requires `<cpu_boost> <gpu_boost>` args        |

Boost levels (Power Mode 4 only):

| Level | CPU                                  | GPU    |
| ----- | ------------------------------------ | ------ |
| 0     | Low                                  | Low    |
| 1     | Normal                               | Normal |
| 2     | High                                 | High   |
| 3     | Boost (models with `boost` feature)  | -      |

### Keyboard Effects

Standard effects (firmware-side). Invoke as
`razer-cli standard-effect <name> <args>`:

| Effect      | Args                                                   | Notes                              |
| ----------- | ------------------------------------------------------ | ---------------------------------- |
| `off`       | -                                                      | No lighting                        |
| `wave`      | `<direction>`                                          | 0 or 1                             |
| `spectrum`  | -                                                      | Color cycle                        |
| `static`    | `<r> <g> <b>`                                          | Solid color                        |
| `reactive`  | `<speed> <r> <g> <b>`                                  | Speed 0–255                        |
| `breathing` | `<kind> <r1> <g1> <b1> <r2> <g2> <b2>`                 | Kind: 0=single, 1=dual, 2=random   |
| `starlight` | `<kind> <speed> <r1> <g1> <b1> <r2> <g2> <b2>`         | Kind same as `breathing`           |

Custom per-key effects (animated, daemon-side). Invoke as
`razer-cli effect <name> <args>`:

| Effect              | Args                                   |
| ------------------- | -------------------------------------- |
| `static`            | `<r> <g> <b>`                          |
| `static-gradient`   | `<r1> <g1> <b1> <r2> <g2> <b2>`        |
| `wave-gradient`     | `<r1> <g1> <b1> <r2> <g2> <b2>`        |
| `breathing-single`  | `<r> <g> <b> <duration>`               |

### Basic Examples

| Goal                                 | Command                                       |
| ------------------------------------ | --------------------------------------------- |
| Balanced power on AC                 | `razer-cli write power ac 0`                  |
| Gaming power on AC                   | `razer-cli write power ac 1`                  |
| Silent power on AC                   | `razer-cli write power ac 3`                  |
| Custom power (CPU=2, GPU=2) on AC    | `razer-cli write power ac 4 2 2`              |
| Solid red keyboard (firmware)        | `razer-cli standard-effect static 255 0 0`    |
| Animated red (daemon)                | `razer-cli effect static 255 0 0`             |
| Read current fan, AC                 | `razer-cli read fan ac`                       |
| Enable BHO at 80%                    | `razer-cli write bho on 80`                   |
| List detected devices                | `razer-cli device-info`                       |

## Troubleshooting

When having problems with the application, please share the following
information:

- Journal: `journalctl --user-unit razercontrol > razercontrol.journal.log`
- Output from `razer-cli device-info`
- Output from `systemctl --user status razercontrol`

## Unofficial Razer Linux Channel

You can find support on the following discord server, under the
'razer-laptop-control' channel: [Discord link](https://discord.gg/GdHKf45).
