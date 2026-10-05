const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFile, execFileSync, execSync } = require('child_process');
const { promisify } = require('util');

const execFileAsync = promisify(execFile);
let lastCpuSnapshot = null;

function getSystemFonts() {
    try {
        if (process.platform === 'win32') {
            const out = execSync(
                'powershell -NoProfile -Command "[System.Reflection.Assembly]::LoadWithPartialName(\'System.Drawing\') | Out-Null; (New-Object System.Drawing.Text.InstalledFontCollection).Families | ForEach-Object { $_.Name }"',
                { encoding: 'utf-8', timeout: 10000, windowsHide: true },
            );
            return out.split('\n').map((entry) => entry.trim()).filter(Boolean).sort();
        }

        const out = execSync('fc-list --format="%{family[0]}\\n" | sort -u', {
            encoding: 'utf-8',
            timeout: 10000,
        });
        return out.split('\n').map((entry) => entry.trim()).filter(Boolean);
    } catch {
        return [
            'Cascadia Code',
            'Cascadia Mono',
            'Consolas',
            'JetBrains Mono',
            'Fira Code',
            'Source Code Pro',
            'Hack',
            'DejaVu Sans Mono',
            'Ubuntu Mono',
            'Courier New',
            'monospace',
        ];
    }
}

function getAvailableShells() {
    const shells = [];
    try {
        if (process.platform === 'win32') {
            const systemRoot = process.env.SystemRoot || 'C:\\Windows';
            const windowsShells = [
                {
                    name: 'Windows PowerShell',
                    path: path.join(systemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe'),
                },
                { name: 'Command Prompt', path: path.join(systemRoot, 'System32', 'cmd.exe') },
            ];

            try {
                const pwshPath = execFileSync('where.exe', ['pwsh.exe'], {
                    encoding: 'utf-8',
                    timeout: 5000,
                    windowsHide: true,
                }).split('\n')[0].trim();
                if (pwshPath) {
                    shells.push({ name: 'PowerShell 7', path: pwshPath });
                }
            } catch {}

            for (const shellDef of windowsShells) {
                if (fs.existsSync(shellDef.path)) {
                    shells.push(shellDef);
                }
            }

            try {
                const wslOut = execFileSync('wsl.exe', ['-l', '-q'], {
                    encoding: 'utf-16le',
                    timeout: 5000,
                    windowsHide: true,
                });
                const distros = wslOut.split('\n')
                    .map((entry) => entry.replace(/\0/g, '').trim())
                    .filter(Boolean);
                if (distros.length > 0) {
                    shells.push({ name: 'WSL (default)', path: 'wsl' });
                }
                for (const distro of distros) {
                    shells.push({ name: `WSL: ${distro}`, path: 'wsl', args: `-d ${distro}` });
                }
            } catch {}
        } else {
            try {
                const content = fs.readFileSync('/etc/shells', 'utf-8');
                const shellPaths = content.split('\n')
                    .map((line) => line.trim())
                    .filter((line) => line && !line.startsWith('#'));
                for (const shellPath of shellPaths) {
                    if (fs.existsSync(shellPath)) {
                        shells.push({ name: path.basename(shellPath), path: shellPath });
                    }
                }
            } catch {}

            if (shells.length === 0 && process.env.SHELL) {
                shells.push({ name: path.basename(process.env.SHELL), path: process.env.SHELL });
            }
        }
    } catch {
        // Return whatever we collected so far.
    }
    return shells;
}

function aggregateCpuTimes() {
    const cpus = os.cpus();
    let idle = 0;
    let total = 0;

    for (const cpu of cpus) {
        idle += cpu.times.idle;
        total += cpu.times.user + cpu.times.nice + cpu.times.sys + cpu.times.idle + cpu.times.irq;
    }

    return { idle, total };
}

function getCpuUsagePercent() {
    const current = aggregateCpuTimes();

    if (!lastCpuSnapshot) {
        lastCpuSnapshot = current;
        return 0;
    }

    const totalDelta = current.total - lastCpuSnapshot.total;
    const idleDelta = current.idle - lastCpuSnapshot.idle;
    lastCpuSnapshot = current;

    if (totalDelta <= 0) {
        return 0;
    }

    return Number((((totalDelta - idleDelta) / totalDelta) * 100).toFixed(1));
}

async function getSwapStats() {
    try {
        if (process.platform === 'linux') {
            const { stdout } = await execFileAsync('free', ['-b'], { encoding: 'utf8', timeout: 5000 });
            const swapLine = stdout.split('\n').find((line) => line.trim().startsWith('Swap:'));
            if (!swapLine) return null;

            const parts = swapLine.trim().split(/\s+/);
            return {
                totalBytes: Number(parts[1] || 0),
                usedBytes: Number(parts[2] || 0),
                freeBytes: Number(parts[3] || 0),
            };
        }
    } catch {
        return null;
    }

    return null;
}

function readSysfsGpuStats() {
    const drmRoot = '/sys/class/drm';
    if (!fs.existsSync(drmRoot)) return [];
    const gpus = [];
    let entries = [];
    try {
        entries = fs.readdirSync(drmRoot);
    } catch {
        return [];
    }
    for (const entry of entries) {
        if (!/^card\d+$/.test(entry)) continue;
        const busyPath = path.join(drmRoot, entry, 'device', 'gpu_busy_percent');
        try {
            const utilizationPercent = Number(fs.readFileSync(busyPath, 'utf8').trim());
            if (!Number.isFinite(utilizationPercent)) continue;
            gpus.push({
                id: entry,
                name: entry,
                memoryUsedMB: null,
                memoryTotalMB: null,
                utilizationPercent,
            });
        } catch {
            // This card does not publish a busy counter.
        }
    }
    return gpus;
}

async function getGpuStats({ detailed = false } = {}) {
    if (!detailed) return readSysfsGpuStats();
    try {
        const { stdout } = await execFileAsync(
            'nvidia-smi',
            ['--query-gpu=name,memory.used,memory.total,utilization.gpu', '--format=csv,noheader,nounits'],
            { encoding: 'utf8', timeout: 5000, windowsHide: true },
        );

        return stdout
            .split('\n')
            .map((line) => line.trim())
            .filter(Boolean)
            .map((line, index) => {
                const [name, memoryUsedMB, memoryTotalMB, utilizationPercent] = line.split(',').map((part) => part.trim());
                return {
                    id: `gpu_${index}`,
                    name,
                    memoryUsedMB: Number(memoryUsedMB || 0),
                    memoryTotalMB: Number(memoryTotalMB || 0),
                    utilizationPercent: Number(utilizationPercent || 0),
                };
            });
    } catch {
        return [];
    }
}

async function getTopProcesses(limit = 24) {
    if (limit === 0) return [];
    const safeLimit = Math.max(8, Math.min(64, Number(limit) || 24));

    try {
        if (process.platform === 'win32') {
            const psCommand = `Get-CimInstance Win32_Process | Select-Object ProcessId,Name,WorkingSetSize,CommandLine | Sort-Object WorkingSetSize -Descending | Select-Object -First ${safeLimit} | ConvertTo-Json -Compress`;
            const { stdout } = await execFileAsync('powershell', ['-NoProfile', '-Command', psCommand], {
                encoding: 'utf8',
                timeout: 10000,
                windowsHide: true,
            });
            const trimmed = stdout.trim();
            if (!trimmed) return [];

            const parsed = JSON.parse(trimmed);
            const items = Array.isArray(parsed) ? parsed : [parsed];

            return items.map((item) => ({
                pid: Number(item.ProcessId || 0),
                name: String(item.Name || 'unknown'),
                cpuPercent: null,
                memoryBytes: Number(item.WorkingSetSize || 0),
                state: 'running',
                command: String(item.CommandLine || item.Name || ''),
            }));
        }

        const { stdout } = await execFileAsync(
            'sh',
            ['-c', `ps -eo pid=,comm=,%cpu=,rss=,state=,args= --sort=-%cpu | head -n ${safeLimit + 1}`],
            { encoding: 'utf8', timeout: 10000 },
        );

        return stdout
            .split('\n')
            .map((line) => line.trim())
            .filter(Boolean)
            .map((line) => {
                const match = line.match(/^(\d+)\s+(\S+)\s+([\d.]+)\s+(\d+)\s+(\S+)\s+(.*)$/);
                if (!match) return null;

                return {
                    pid: Number(match[1]),
                    name: match[2],
                    cpuPercent: Number(match[3]),
                    memoryBytes: Number(match[4]) * 1024,
                    state: match[5],
                    command: match[6],
                };
            })
            .filter(Boolean);
    } catch {
        return [];
    }
}

async function getSystemMonitorSnapshot(options = {}) {
    const cpus = os.cpus();
    const totalMemoryBytes = os.totalmem();
    const freeMemoryBytes = os.freemem();
    const usedMemoryBytes = totalMemoryBytes - freeMemoryBytes;
    const processLimit = options && typeof options === 'object' ? options.processLimit : undefined;
    const skipProcessList = processLimit === 0;
    const detailedGpu = options && options.gpuDetails === true;

    const [swap, gpus, processes] = await Promise.all([
        skipProcessList ? null : getSwapStats(),
        getGpuStats({ detailed: detailedGpu }),
        skipProcessList ? [] : getTopProcesses(processLimit),
    ]);

    return {
        timestamp: Date.now(),
        platform: process.platform,
        hostname: os.hostname(),
        uptimeSeconds: Math.round(os.uptime()),
        cpu: {
            usagePercent: getCpuUsagePercent(),
            coreCount: cpus.length,
            model: cpus[0]?.model || 'unknown',
            loadAverage: os.loadavg().map((value) => Number(value.toFixed(2))),
        },
        memory: {
            totalBytes: totalMemoryBytes,
            usedBytes: usedMemoryBytes,
            freeBytes: freeMemoryBytes,
            swapTotalBytes: swap?.totalBytes ?? null,
            swapUsedBytes: swap?.usedBytes ?? null,
            swapFreeBytes: swap?.freeBytes ?? null,
        },
        gpus,
        processes,
    };
}

module.exports = {
    getAvailableShells,
    getSystemFonts,
    getSystemMonitorSnapshot,
};
