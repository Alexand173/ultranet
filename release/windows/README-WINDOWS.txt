UltraNet Node Windows x64 package
=================================

This package is for 64-bit Windows (x86_64). It contains:

  UltraNetNode.exe          the node executable
  Start-UltraNetNode.bat    desktop-friendly launcher
  UltraNetNode.env.example  safe configuration template
  README-WINDOWS.txt        this guide

Do not use these files on ARM or another unsupported architecture. Do not put
private keys, wallet backups, or a real environment file in the archive.

1. Verify the download
----------------------

Download SHA256SUMS.txt from the same GitHub release. In PowerShell, run:

  $expected = (Get-Content .\SHA256SUMS.txt | Where-Object { $_ -match 'UltraNetNode-windows-x64\.zip$' }).Split()[0]
  $actual = (Get-FileHash .\UltraNetNode-windows-x64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -ne $expected) { throw "Checksum mismatch" }
  "Checksum OK"

Do not extract or execute an archive after a checksum mismatch.

2. Extract and create the private configuration file
------------------------------------------------------

Extract the complete archive into a folder that you can write to. In that
folder, copy:

  UltraNetNode.env.example  ->  UltraNetNode.env

UltraNetNode.env is read only from the local machine. Keep it private and do
not upload it, commit it, email it, or paste its contents into a browser.
The launcher sets ULTRANET_ENV_FILE to this sibling file. An already-set
process or service variable always takes precedence over the file.

3. Create ULTRANET_ADMIN_TOKEN
------------------------------

ULTRANET_ADMIN_TOKEN is a private administrator bearer token for state-changing
node operations such as mining, pruning, and AppChain management. It is not a
wallet key, a public node identifier, a DILITHIUM_PUB_KEY, or an ordinary-user
login token. The node requires it before the API can start; it is never
generated automatically and must never be exposed to website code.

OpenSSL (Git for Windows, OpenSSL, or another trusted local installation):

  openssl rand -hex 32

PowerShell (creates 32 random bytes / 64 hexadecimal characters):

  $bytes = New-Object byte[] 32
  $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
  $rng.GetBytes($bytes)
  $token = [BitConverter]::ToString($bytes).Replace('-', '').ToLowerInvariant()
  $rng.Dispose()
  $token

Copy the printed value into UltraNetNode.env on this line:

  ULTRANET_ADMIN_TOKEN=your-64-hex-character-value

Use a fresh value for each node. Never use a wallet secret or public key as the
admin token. If the token is lost, create a new one and restart the node.

4. First launch
---------------

For a normal desktop launch, double-click Start-UltraNetNode.bat. The launcher:

  * starts in the folder containing the package;
  * loads the sibling UltraNetNode.env file;
  * runs --check-config before storage and cryptographic initialization;
  * keeps configuration failures visible and returns a non-zero exit code; and
  * starts UltraNetNode.exe only after configuration succeeds.

When an expected configuration error occurs, the console remains open until
Enter is pressed. The node uses this writable default data directory:

  %LOCALAPPDATA%\UltraNet\data

Leave ULTRANET_DB_PATH commented to use that default. If you choose another
path, create it first and use a real absolute Windows path in UltraNetNode.env.
Do not place the database under a protected system directory.

A terminal launch is also supported:

  .\UltraNetNode.exe --check-config
  .\UltraNetNode.exe

Set ULTRANET_PAUSE_ON_ERROR=false in that terminal when you do not want a
failure pause. Do not enable the pause for services, containers, CI, or other
non-interactive launches.

5. Networking and firewall
--------------------------

The local API defaults to 127.0.0.1:8081. Keep that port private unless you
understand the TLS reverse-proxy and CORS configuration. Set
ULTRANET_CORS_ORIGINS to the exact http(s) origin that is allowed to call the
API; wildcard origins are rejected.

The validator P2P listener uses port 9000. If this node must accept peers,
allow inbound TCP and UDP 9000 in Windows Defender Firewall only for the
network profile and scope you intend to use. Do not expose the API port to the
public internet just to make P2P work.

6. Logs and troubleshooting
---------------------------

Keep the console window open while reproducing a launch problem. Copy the
English error text, the command used, the Windows version, and the package
version. Never include UltraNetNode.env, ULTRANET_ADMIN_TOKEN, private keys, or
the database directory in a bug report.

The most common startup message is:

  ULTRANET_ADMIN_TOKEN is required for the node API

This is a configuration error, not a peer or firewall error. Create the token
as shown above, save UltraNetNode.env beside the executable, and run the
launcher again. For a service deployment, configure the token in the service
environment instead of using this desktop file.

For service or Docker deployments, keep their environment handling separate:

  * systemd reads deploy/ultranet.env through EnvironmentFile;
  * the repository's local docker-compose.yml requires
    ${ULTRANET_ADMIN_TOKEN:?Set ULTRANET_ADMIN_TOKEN before starting the node};
  * production Compose uses its configured env_file and the node's preflight;
    and
  * a sibling UltraNetNode.env is intended for this interactive desktop package.

Do not put the administrator token in Next.js NEXT_PUBLIC_* variables, frontend
source, URLs, local storage, or screenshots.
