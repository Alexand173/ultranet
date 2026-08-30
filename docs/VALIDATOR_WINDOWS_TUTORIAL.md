# UltraNet Windows Validator Tutorial

**Target length:** 2–3 minutes
**Audience:** a first-time Windows x64 validator operator
**Release:** v7.1.6
**Recording status:** production script and shot list; capture the real Windows session before publishing

This script is intentionally written for a real Windows desktop recording. Do not replace the node launch, checksum, or Genesis connection with a mock terminal. The Linux development workspace cannot capture the Windows desktop or produce a truthful runtime recording.

## Safety before recording

- Use a clean Windows x64 test folder and the published `UltraNetNode-windows-x64.zip` from the [v7.1.6 release](https://github.com/Alexand173/ultranet/releases/tag/v7.1.6).
- Keep the recording at 1920×1080, 30 fps. Record the browser, File Explorer, PowerShell, Notepad, and the node console; do not record unrelated desktop notifications.
- Prepare a real token locally, but pause the recording or crop/obscure the value while generating, entering, and saving `ULTRANET_ADMIN_TOKEN`.
- Do not show private keys, wallet recovery words, `UltraNetNode.env` contents, database paths, personal usernames, SSH material, or browser cookies.
- The public Genesis P2P multiaddr may be shown:

  ```text
  /ip4/167.233.161.115/tcp/9000/p2p/12D3KooWAa2qGYoTkke8Sdfixo1jLXiqCbNFksU1bUrYhNAoAjHb
  ```

- The node API uses local TCP `127.0.0.1:8081`; do not expose it publicly just to make P2P work. Validator peers use TCP and UDP port `9000`.

## Recording setup

### OBS

1. Create a scene named `UltraNet validator onboarding`.
2. Add a window capture for the browser and a window/display capture for the Windows terminal.
3. Set the base and output resolution to 1920×1080, 30 fps.
4. Hide notifications and use a readable terminal font. Keep the cursor visible when clicking the launcher.
5. Add a small lower-third only if desired: `UltraNet v7.1.6 // Windows x64`.

### Windows Game Bar

Press `Win+G`, enable microphone only if narration is required, and capture the browser, File Explorer, PowerShell, Notepad, and node console in one continuous desktop flow. Crop or stop capture before any secret is visible.

## Shot list and narration

| Time | Screen action | Narration | Evidence to capture |
| --- | --- | --- | --- |
| `0:00–0:15` | Open `https://ultranetwork.cc/validator` or `/download` and point at the v7.1.6 Windows x64 link. | “This is the UltraNet v7.1.6 validator onboarding path. I’ll download the verified Windows x64 node, start it, and confirm that it reaches Genesis.” | Release tag `v7.1.6`, Windows x64 archive, and checksum link. |
| `0:15–0:35` | Download `UltraNetNode-windows-x64.zip` and `SHA256SUMS.txt` into a clean folder. Open PowerShell in that folder. | “I download the archive and its checksum manifest from the same GitHub release. I verify before extracting.” | Both files visible; no personal path or unrelated downloads. |
| `0:35–0:50` | Run the checksum command below. Keep the result on screen. | “The hash matches, so this is the package I intend to run.” | `Checksum OK`. Stop and fix the download if the result is `Checksum mismatch`. |
| `0:50–1:05` | Extract the complete archive with File Explorer. Show the four package files briefly. | “The complete package includes the executable, launcher, safe environment template, and Windows guide.” | `UltraNetNode.exe`, `Start-UltraNetNode.bat`, `UltraNetNode.env.example`, `README-WINDOWS.txt`. |
| `1:05–1:35` | Double-click `Start-UltraNetNode.bat`. When Notepad opens the newly created env file, pause/crop the recording before entering the token. Generate the token locally with the PowerShell command below, enter it privately, save, and close Notepad. | “On first run the launcher creates the private sibling environment file. I generate a fresh 32-byte token locally. This is an administrator token for my node—not a wallet key—and it never belongs in website code or a screenshot.” | The launcher message that it created `UltraNetNode.env`; never the token value or env contents. |
| `1:35–1:55` | Let the launcher continue. Show the two preflight commands completing. | “Before storage and node startup, the launcher checks configuration and Windows FHE initialization.” | `UltraNetNode configuration is valid.` and `UltraNet FHE initialization is valid.` |
| `1:55–2:25` | Keep the console visible while the node starts. Scroll only enough to show startup and connection lines. | “The node is now running. It loads the persistent identity, adds the Genesis bootnode, and performs the libp2p handshake.” | `P2P Node running!`, `Peer ID: ...`, `Added Bootnode: ...`, and `libp2p connection established: ...`. Redact any local path if needed. |
| `2:25–2:45` | Wait for one or two recurring heartbeat lines. Then show the validator page’s proposal handoff without submitting a real secret. | “Heartbeat confirms the node is still participating. The final step is a signed proposal: UltraWallet signs locally, and 2-of-3 Sovereign approval is required before activation.” | `Heartbeat - PeerManager tracked peers: ...; libp2p connected peers: ...` and the public proposal handoff. |

## Exact PowerShell snippets

Run these in the folder containing the downloaded files. Never type a real token into the recording while capture is active.

### Verify the Windows archive

```powershell
$expected = (Get-Content .\\SHA256SUMS.txt | Where-Object { $_ -match 'UltraNetNode-windows-x64\\.zip$' }).Split()[0]
$actual = (Get-FileHash .\\UltraNetNode-windows-x64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "Checksum mismatch" }
"Checksum OK"
```

### Generate the private administrator token off-camera

```powershell
$bytes = New-Object byte[] 32
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$rng.GetBytes($bytes)
$token = [BitConverter]::ToString($bytes).Replace('-', '').ToLowerInvariant()
$rng.Dispose()
$token
```

Paste the value only into the private local `UltraNetNode.env` file. Do not use a wallet key, public node identifier, short password, or previously reused value as the admin token.

### Optional terminal launch for a retake

The supplied launcher is preferred because it runs both preflight checks and preserves interactive failures:

```powershell
.\\Start-UltraNetNode.bat
```

For a controlled troubleshooting retake, the equivalent executable commands are:

```powershell
.\\UltraNetNode.exe --check-config
.\\UltraNetNode.exe --check-fhe
.\\UltraNetNode.exe
```

## Expected console evidence

The exact peer count and block height may differ, but the recording should show these categories of messages:

```text
UltraNet configuration is valid.
UltraNet FHE initialization is valid.
P2P Node running!
Added Bootnode: 12D3KooWAa2qGYoTkke8Sdfixo1jLXiqCbNFksU1bUrYhNAoAjHb
libp2p connection established: peer=...
Heartbeat - PeerManager tracked peers: ...; libp2p connected peers: ...
```

Do not claim “validator active” merely because the binary started. The node must still complete the signed proposal and governance approval process.

## Publishing checklist

- [ ] The recording is between 2 and 3 minutes and uses the real v7.1.6 Windows x64 package.
- [ ] The checksum command visibly returns `Checksum OK`.
- [ ] The complete archive and launcher are shown.
- [ ] The launcher-created `UltraNetNode.env` is acknowledged without exposing its contents.
- [ ] The admin token is generated and entered off-camera or fully redacted.
- [ ] `--check-config` and `--check-fhe` both pass.
- [ ] Node startup, Genesis bootnode, libp2p connection, and repeated Heartbeat evidence are visible.
- [ ] No private key, recovery phrase, database path, cookie, SSH key, or real token appears.
- [ ] The final frame explains that proposal submission and 2-of-3 Sovereign approval are still required.
