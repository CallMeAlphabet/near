# near

near — a LAN-only file transfer hub

## Table of Contents

- [Quick Start](#quick-start)
- [Uninstall](#uninstall)
- [Usage](#usage)
- [Features](#features)
- [Example Output](#example-output)
- [How It Works](#how-it-works)
- [Testing Conditions](#testing-conditions)

## Quick Start
- **On Arch**
```bash
paru -S near
```
- **On Nix**
```bash
nix run github:CallMeAlphabet/near
```
To keep it installed:
```bash
nix profile install github:CallMeAlphabet/near
```
- **Non-arch**
```bash
cargo install near-lan
```
> **Note**: The crate is `near-lan`, not `near`. Somebody published an empty `near` crate on crates.io in 2021. The binary is still called `near`. Make sure `~/.cargo/bin` is in your `PATH`. It's added automatically by rustup, but if `near` isn't found, add this to your shell config file:
> ```bash
> # If you use Bash:
> export PATH="$HOME/.cargo/bin:$PATH"
>
> # If you use Fish:
> fish_add_path $HOME/.cargo/bin
>
> # If you use Zsh:
> export PATH="$PATH:$HOME/.cargo/bin"
> ```

```bash
near
```

On first run near writes a local CA to `~/.config/near`. Import `ca.crt` into the trust store of every device you want to use, after comparing the fingerprint printed at startup.

```bash
# Linux
sudo trust anchor ~/.config/near/ca.crt

# macOS
sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain ~/.config/near/ca.crt

# Windows, from an administrator shell
certutil -addstore -f ROOT %USERPROFILE%\.config\near\ca.crt
```

Then open the address near prints on both devices.

## Uninstall

```bash
cargo uninstall near-lan
```
```bash
paru -R near
```
```bash
nix profile remove near
```

The CA is installed in your OS trust store and the generated files stay in `~/.config/near`, so delete both. Remove the CA first, while `ca.crt` still exists:

```bash
# Linux
sudo trust anchor --remove ~/.config/near/ca.crt

# macOS
sudo security delete-certificate -c "near local ca" /Library/Keychains/System.keychain

# Windows, from an administrator shell
certutil -delstore ROOT "near local ca"
```

Then delete the files:

```bash
rm -rf ~/.config/near
```

On Windows use `rmdir /s /q %USERPROFILE%\.config\near` instead. If you used `--cert-dir`, delete that directory instead.

## Usage

### Basic usage

```bash
near

near --port 8080

near --host 192.168.1.20

near --cert-dir ./certs

near --web-dir ./dist

near --print-ca
```

Open `https://<lan-ip>:53317/near` on both devices. Both land in the same room, see each other after a moment, and one side clicks `pair`. Then choose files, paste text if you want, and click `send`. The receiving side gets a `save` button when the integrity check finishes.

### Common use cases

```bash
# keep the hub on one machine and use a fixed port
near --port 53317

# check the CA fingerprint before importing it on a phone
near --print-ca

# serve a built copy of the site instead of the bundled page
near --web-dir ./dist
```

`near` does not need a public DNS record, an account, or an internet connection. If the hub address is entered manually, the page will refuse anything that is not a local address.

## Features

### Network

- Works with the WAN unplugged. No STUN, TURN, public signaling, cloud storage, accounts, or telemetry.
- A local CA is generated on first start. No external tool is needed. The leaf certificate carries SANs for `near.local`, `localhost`, `127.0.0.1`, and every non-loopback IPv4 address found on the machine.
- Signaling is long-poll over HTTPS. Rooms and messages live in memory and are capped at 500 messages each.
- The hub only sees SDP and ICE messages. File bytes go between the two browsers.

### Transfer

- One ordered WebRTC data channel, 256 KiB chunks, and backpressure at 4 MiB of buffered data.
- Each chunk gets a SHA-256 digest and an acknowledgement. The final digest covers the concatenated chunk digests, so a changed or missing chunk fails the check.
- Both sides keep the chunk digests and their position for the session. If the channel drops, sending the same file again starts from the last chunk the other device acknowledged.

### Page

- The bundled page has no external assets, scripts, or fonts from outside the hub.
- `--web-dir` serves a built copy of the website instead, including its `_next/static` files.

## Example Output

```
near 0.1.0
ca: /home/you/.config/near/ca.crt
ca sha256: 97:BE:67:24:59:19:2C:12:2B:73:07:84:76:A9:FD:77:75:73:AA:A4:8F:E3:E0:C2:AE:F7:CF:8D:62:90:F9:C0
open on this lan:
  https://192.168.1.20:53317/near
  https://near.local:53317/near
listening on 0.0.0.0:53317
```

Your address and fingerprint will differ. Check the fingerprint against `near --print-ca` before trusting the CA.

## How It Works:
  1. `near` starts an HTTPS listener with a certificate signed by a CA it generated locally. Nothing is fetched from outside the LAN.
  2. Each device joins a room with `POST /api/join` and waits on `GET /api/poll`. Offers, answers, and ICE candidates pass through the hub as JSON.
  3. The page opens `RTCPeerConnection({ iceServers: [] })`. Only host candidates are gathered, so both peers have to be on the same network.
  4. One ordered data channel carries control frames as JSON strings and file chunks as raw bytes. The sender waits on `bufferedAmountLowThreshold` when the buffer gets full.
  5. The receiver acknowledges every chunk, keeps its digests, and compares a final digest when the sender says it is done. The progress bar moves on those acknowledgements.

### Full Help

```
LAN-only file transfer hub

Usage: near [OPTIONS]

Options:
  -p, --port <PORT>          Port to listen on [default: 53317]
  -H, --host <HOST>          Address to bind [default: 0.0.0.0]
  -c, --cert-dir <CERT_DIR>  Directory for the generated certificate authority
  -w, --web-dir <WEB_DIR>    Serve a built site from this directory instead of the bundled page
      --print-ca             Print the CA path and fingerprint, then exit
  -h, --help                 Print help
  -V, --version              Print version
```

## Testing Conditions

Two devices on the same LAN, each with `ca.crt` in its trust store, and the hub address opened directly. WebRTC gathers host candidates only, so both devices must be able to reach each other directly; AP isolation will block it. The WAN can be unplugged. No benchmark numbers are published for near.
