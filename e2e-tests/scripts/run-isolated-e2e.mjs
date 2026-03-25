import { spawn } from 'node:child_process'
import { createWriteStream } from 'node:fs'
import { mkdtemp, mkdir, readFile, rm } from 'node:fs/promises'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import process from 'node:process'
import { fileURLToPath } from 'node:url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const repoRoot = path.resolve(__dirname, '..', '..')
const e2eRoot = path.resolve(repoRoot, 'e2e-tests')
const webRoot = path.resolve(repoRoot, 'web')
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm'
const cargoCommand = process.platform === 'win32' ? 'cargo.exe' : 'cargo'

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

function createManagedChild(command, args, options) {
  const child = spawn(command, args, {
    cwd: options.cwd,
    env: options.env,
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: process.platform !== 'win32',
  })

  if (!child.stdout || !child.stderr) {
    throw new Error(`Failed to capture stdio for ${options.label}`)
  }

  const logStream = createWriteStream(options.logFile, { flags: 'a' })
  child.stdout.pipe(logStream)
  child.stderr.pipe(logStream)

  child.on('error', (error) => {
    logStream.write(`\n[${options.label}] spawn error: ${String(error)}\n`)
  })

  return { child, logStream }
}

async function terminateManagedChild(managed, label) {
  if (!managed) {
    return
  }

  const { child, logStream } = managed
  if (child.exitCode === null && child.signalCode === null) {
    try {
      if (process.platform !== 'win32' && child.pid) {
        process.kill(-child.pid, 'SIGTERM')
      } else {
        child.kill('SIGTERM')
      }
    } catch {
      // Ignore termination race.
    }

    const deadline = Date.now() + 5000
    while (Date.now() < deadline) {
      if (child.exitCode !== null || child.signalCode !== null) {
        break
      }
      await delay(100)
    }

    if (child.exitCode === null && child.signalCode === null) {
      try {
        if (process.platform !== 'win32' && child.pid) {
          process.kill(-child.pid, 'SIGKILL')
        } else {
          child.kill('SIGKILL')
        }
      } catch {
        // Ignore kill race.
      }
    }
  }

  await new Promise((resolve) => logStream.end(resolve))
  console.log(`[e2e] stopped ${label}`)
}

async function findFreePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer()
    server.unref()
    server.on('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      if (!address || typeof address === 'string') {
        server.close(() => reject(new Error('Failed to allocate port')))
        return
      }
      const { port } = address
      server.close((error) => {
        if (error) {
          reject(error)
          return
        }
        resolve(port)
      })
    })
  })
}

async function waitForUrl(url, label, timeoutMs, managed) {
  const deadline = Date.now() + timeoutMs

  while (Date.now() < deadline) {
    if (managed?.child.exitCode !== null) {
      throw new Error(`${label} exited early with code ${managed.child.exitCode}`)
    }
    if (managed?.child.signalCode !== null) {
      throw new Error(`${label} exited early with signal ${managed.child.signalCode}`)
    }

    try {
      const response = await fetch(url)
      if (response.ok) {
        return
      }
    } catch {
      // Retry until timeout.
    }

    await delay(250)
  }

  throw new Error(`Timed out waiting for ${label} at ${url}`)
}

async function readLog(filePath) {
  try {
    return await readFile(filePath, 'utf8')
  } catch {
    return ''
  }
}

async function main() {
  const tempRoot = await mkdtemp(path.join(os.tmpdir(), 'restflow-e2e-'))
  const stateDir = path.join(tempRoot, 'state')
  const logsDir = path.join(tempRoot, 'logs')
  const dbPath = path.join(tempRoot, 'restflow-e2e.db')
  const cargoTargetDir = process.env.CARGO_TARGET_DIR || path.join(os.homedir(), '.cargo-targets', 'restflow-e2e')
  const daemonPort = await findFreePort()
  const webPort = await findFreePort()
  const baseUrl = `http://127.0.0.1:${webPort}`
  const daemonBaseUrl = `http://127.0.0.1:${daemonPort}`
  const forwardedArgs = process.argv.slice(2)

  await mkdir(stateDir, { recursive: true })
  await mkdir(logsDir, { recursive: true })
  await mkdir(cargoTargetDir, { recursive: true })

  const daemonLog = path.join(logsDir, 'daemon.log')
  const webLog = path.join(logsDir, 'web.log')

  let daemon
  let web

  const cleanup = async () => {
    await terminateManagedChild(web, 'web')
    await terminateManagedChild(daemon, 'daemon')
    await rm(tempRoot, { recursive: true, force: true })
  }

  const handleSignal = async (signal) => {
    console.error(`[e2e] received ${signal}, cleaning up`) 
    await cleanup()
    process.exit(1)
  }

  process.on('SIGINT', () => {
    void handleSignal('SIGINT')
  })
  process.on('SIGTERM', () => {
    void handleSignal('SIGTERM')
  })

  try {
    console.log(`[e2e] temp root: ${tempRoot}`)
    console.log(`[e2e] daemon port: ${daemonPort}`)
    console.log(`[e2e] web port: ${webPort}`)

    daemon = createManagedChild(
      cargoCommand,
      [
        'run',
        '--package',
        'restflow-cli',
        '--',
        '--db-path',
        dbPath,
        'daemon',
        'start',
        '--foreground',
        '--mcp-port',
        String(daemonPort),
      ],
      {
        cwd: repoRoot,
        env: {
          ...process.env,
          RESTFLOW_DIR: stateDir,
          RESTFLOW_DB_PATH: dbPath,
          CARGO_TARGET_DIR: cargoTargetDir,
        },
        logFile: daemonLog,
        label: 'daemon',
      },
    )

    await waitForUrl(`${daemonBaseUrl}/api/health`, 'daemon', 120000, daemon)
    console.log('[e2e] daemon is ready')

    web = createManagedChild(
      npmCommand,
      ['run', 'dev', '--', '--host', '127.0.0.1', '--port', String(webPort)],
      {
        cwd: webRoot,
        env: {
          ...process.env,
          RESTFLOW_VITE_PROXY_TARGET: daemonBaseUrl,
        },
        logFile: webLog,
        label: 'web',
      },
    )

    await waitForUrl(`${baseUrl}/workspace`, 'web', 60000, web)
    console.log('[e2e] web is ready')

    const playwrightExitCode = await new Promise((resolve) => {
      const child = spawn(
        npmCommand,
        ['run', 'playwright', '--', ...forwardedArgs],
        {
          cwd: e2eRoot,
          env: {
            ...process.env,
            BASE_URL: baseUrl,
            PLAYWRIGHT_HTML_OPEN: 'never',
          },
          stdio: 'inherit',
        },
      )

      child.on('exit', (code, signal) => {
        if (signal) {
          resolve(1)
          return
        }
        resolve(code ?? 1)
      })

      child.on('error', () => resolve(1))
    })

    if (playwrightExitCode !== 0) {
      const [daemonOutput, webOutput] = await Promise.all([readLog(daemonLog), readLog(webLog)])
      if (daemonOutput.trim()) {
        console.error('\n[e2e] daemon log\n')
        console.error(daemonOutput)
      }
      if (webOutput.trim()) {
        console.error('\n[e2e] web log\n')
        console.error(webOutput)
      }
      throw new Error(`Playwright exited with code ${playwrightExitCode}`)
    }
  } catch (error) {
    const [daemonOutput, webOutput] = await Promise.all([readLog(daemonLog), readLog(webLog)])
    if (daemonOutput.trim()) {
      console.error('\n[e2e] daemon log\n')
      console.error(daemonOutput)
    }
    if (webOutput.trim()) {
      console.error('\n[e2e] web log\n')
      console.error(webOutput)
    }
    throw error
  } finally {
    await cleanup()
  }
}

main().catch((error) => {
  console.error(`[e2e] ${error instanceof Error ? error.message : String(error)}`)
  process.exit(1)
})
