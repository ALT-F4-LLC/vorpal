const core = require('@actions/core');
const exec = require('@actions/exec');
const fs = require('fs');
const path = require('path');

async function run() {
    try {
        // Get inputs
        const version = core.getInput('version');
        const useLocalBuild = core.getInput('use-local-build') === 'true';
        const registryBackend = core.getInput('registry-backend') || 'local';
        const registryBackendS3Bucket = core.getInput('registry-backend-s3-bucket');
        const port = core.getInput('port') || '23151';
        const services = core.getInput('services') || 'agent,registry,worker';

        // Step 1: Install Vorpal
        await installVorpal(version, useLocalBuild);

        // Step 2: Setup Vorpal Directories
        await setupVorpalDirectories();

        // Step 3: Generate Vorpal Keys
        await generateVorpalKeys();

        // Step 4: Start Vorpal
        await startVorpal(registryBackend, registryBackendS3Bucket, port, services);

    } catch (error) {
        core.setFailed(error.message);
    }
}

async function installVorpal(version, useLocalBuild) {
    core.info('Installing Vorpal...');

    if (useLocalBuild) {
        core.info('Using local build of vorpal');

        await exec.exec('chmod', ['+x', './dist/vorpal']);

        core.addPath(path.join(process.cwd(), 'dist'));
    } else {
        if (!version) {
            throw new Error("'version' input is required when 'use-local-build' is false.");
        }

        const os = process.platform === 'darwin' ? 'darwin' : 'linux';
        const arch = process.arch === 'x64' ? 'x86_64' : 'aarch64';
        const releaseAsset = `vorpal-${arch}-${os}.tar.gz`;
        const releaseUrl = `https://github.com/ALT-F4-LLC/vorpal/releases/download/${version}/${releaseAsset}`;

        core.info(`Downloading from ${releaseUrl}`);

        await exec.exec('curl', ['-sSL', '-o', releaseAsset, releaseUrl]);
        await exec.exec('tar', ['-xzf', releaseAsset]);
        await exec.exec('rm', [releaseAsset]);
        await exec.exec('chmod', ['+x', 'vorpal']);

        core.addPath(process.cwd());
    }
}

async function setupVorpalDirectories() {
    core.info('Setting up Vorpal directories...');

    await exec.exec('sudo', ['mkdir', '-pv', '/var/lib/vorpal/{key,sandbox,store}']);
    await exec.exec('sudo', ['mkdir', '-pv', '/var/lib/vorpal/store/artifact/{alias,archive,config,output}']);
    await exec.exec('sudo', ['chown', '-R', `${process.getuid()}:${process.getgid()}`, '/var/lib/vorpal']);
}

async function generateVorpalKeys() {
    core.info('Generating Vorpal keys...');

    await exec.exec('vorpal', ['system', 'keys', 'generate']);
}

async function startVorpal(registryBackend, registryBackendS3Bucket, port, services) {
    core.info('Starting Vorpal service...');

    // Build command arguments
    const args = [
        'services', 'start',
        '--port', port,
        '--services', services,
        '--registry-backend', registryBackend
    ];

    // Add S3 bucket if S3 backend is specified
    if (registryBackend === 's3') {
        if (!registryBackendS3Bucket) {
            throw new Error('registry-backend-s3-bucket is required when using s3 backend');
        }

        args.push('--registry-backend-s3-bucket', registryBackendS3Bucket);
    }

    const command = `vorpal ${args.join(' ')}`;

    core.info(`Starting vorpal with command: ${command}`);

    // Start the service in background
    const child = exec.spawn('vorpal', args, {
        stdio: ['ignore', 'pipe', 'pipe'],
        detached: true
    });

    // Write logs to file
    const logFile = '/tmp/vorpal_output.log';
    const logStream = fs.createWriteStream(logFile);

    child.stdout.pipe(logStream);
    child.stderr.pipe(logStream);

    // Give it a moment to start
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Check if process is still running
    if (child.killed || child.exitCode !== null) {
        const logs = fs.readFileSync(logFile, 'utf8');
        core.error('Vorpal service failed to start');
        core.error('Service output:');
        core.error(logs);
        throw new Error('Vorpal service failed to start');
    }

    core.info(`Vorpal service is running (PID: ${child.pid})`);

    // Store PID for cleanup
    core.saveState('vorpal-pid', child.pid);

    // Show initial logs
    const logs = fs.readFileSync(logFile, 'utf8');

    core.info('Initial service logs:');
    core.info(logs);
}

run();
