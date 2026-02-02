class PoW {
    constructor() {
        this.busy = false;
        this.isRunning = false;
        this.shouldStop = false;
        this.hashWasm = null;
        this.hasWebCrypto = window.crypto && window.crypto.subtle;
    }

    async init() {
        // wait for hash-wasm to be available
        let attempts = 0;
        while (attempts < 30) {
            if (window.hashWasm || window.HashWasm || (window.hashwasm && window.hashwasm.createBLAKE3)) {
                this.hashWasm = window.hashWasm || window.HashWasm || window.hashwasm;
                log('hash-wasm library loaded successfully', 'success');
                return;
            }

            // check if individual functions are available
            if (window.createBLAKE3 && window.createXXHash3 && window.createSHA256) {
                log('hash-wasm functions detected individually', 'success');
                this.hashWasm = {
                    blake3: async (data) => {
                        const hasher = await window.createBLAKE3();
                        hasher.update(data);
                        return hasher.digest('hex');
                    },
                    xxhash3: async (data) => {
                        const hasher = await window.createXXHash3();
                        hasher.update(data);
                        return hasher.digest('hex');
                    },
                    sha256: async (data) => {
                        const hasher = await window.createSHA256();
                        hasher.update(data);
                        return hasher.digest('hex');
                    }
                };
                return;
            }

            await new Promise(resolve => setTimeout(resolve, 100));
            attempts++;
        }
        log('hash-wasm library not detected! Using Web Crypto API for SHA-256 only', 'warning');

        if (!this.hasWebCrypto) {
            throw new Error('No hash algorithms available');
        }
    }

    async sha256WebCrypto(message) {
        const encoder = new TextEncoder();
        const data = encoder.encode(message);
        const hashBuffer = await crypto.subtle.digest('SHA-256', data);
        const hashArray = Array.from(new Uint8Array(hashBuffer));
        return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
    }

    isAlgorithmAvailable(algorithm) {
        switch (algorithm) {
            case 'blake3':
            case 'xxhash3':
                return this.hashWasm && (this.hashWasm.blake3 || this.hashWasm.xxhash3);
            case 'sha256':
                return this.hashWasm || this.hasWebCrypto;
            default:
                return false;
        }
    }

    hashMeetsTarget(hash, target, algorithm) {
        switch (algorithm) {
            case 'blake3':
            case 'sha256':
                return hash < target;

            case 'xxhash3':
                const hashValue = parseInt(hash.substring(0, 8), 16);
                return hashValue < target;

            default:
                return false;
        }
    }

    async hashMessage(message, algorithm) {
        switch (algorithm) {
            case 'blake3':
                if (this.hashWasm && this.hashWasm.blake3) {
                    return await this.hashWasm.blake3(message);
                }
                throw new Error('BLAKE3 not available');
            case 'xxhash3':
                if (this.hashWasm && this.hashWasm.xxhash3) {
                    return await this.hashWasm.xxhash3(message);
                }
                throw new Error('xxHash3 not available');
            case 'sha256':
                if (this.hashWasm && this.hashWasm.sha256) {
                    return await this.hashWasm.sha256(message);
                } else if (this.hasWebCrypto) {
                    return await this.sha256WebCrypto(message);
                }
                throw new Error('SHA-256 not available');
            default:
                throw new Error(`Unknown algorithm: ${algorithm}`);
        }
    }

    async computePoW(userAddress, seed, target, algorithm, maxAttempts = 5000000) {
        const startTime = performance.now();

        for (let nonce = 0; nonce < maxAttempts; nonce++) {
            if (this.shouldStop) {
                return { success: false, timeMs: performance.now() - startTime, attempts: nonce };
            }
            const message = userAddress + seed + nonce;

            try {
                const hash = await this.hashMessage(message, algorithm);

                if (this.hashMeetsTarget(hash, target, algorithm)) {
                    const endTime = performance.now();
                    return {
                        nonce,
                        hash,
                        attempts: nonce + 1,
                        timeMs: endTime - startTime,
                        success: true
                    };
                }
            } catch (error) {
                return {
                    success: false,
                    error: error.message,
                    timeMs: performance.now() - startTime,
                    attempts: nonce
                };
            }
        }
        return {
            success: false,
            timeMs: performance.now() - startTime,
            attempts: maxAttempts
        };
    }

    async handleChallenge(challenge) {
        const userAddress = challenge.user_address;
        const seed = challenge.seed;
        const difficulty = challenge.difficulty;
        const timestamp = challenge.timestamp;

        const result = await this.computePoW(userAddress, seed, difficulty, 'sha256');

        if (result) {
            console.log("Found valid PoW:", result);
            return {
                nonce: result.nonce,
                hash: result.hash,
                timestamp_ms: timestamp
            }
        } else {
            console.log("Failed to compute valid PoW.");
        }
    }
}

const pow = new PoW();

document.getElementById("send-heartbeat-form").addEventListener("submit", async function (e) {
    e.preventDefault(); // stop normal form submit

    if (pow.busy) {
        return;
    }
    pow.busy = true;

    const ws = new WebSocket("/api/pow");

    document.getElementsByClassName("auth-feedback")[0].id = "";
    let feedback_container = document.getElementsByClassName("auth-feedback")[0];
    let feedback_text = document.getElementById("auth-feedback-text");

    feedback_container.style.backgroundColor = "#7c7402";
    feedback_text.textContent = "Waiting for Challenge from Server..";

    ws.onopen = function () {
        console.log("Connected to challenge stream.");
    };

    ws.onmessage = async function (event) {
        if (pow.isRunning) {
            return;
        }
        pow.isRunning = true;

        const challenge = JSON.parse(event.data);
        console.log("Received challenge:", challenge);

        // solve PoW challenge
        feedback_container.style.backgroundColor = "#7c7402";
        feedback_text.textContent = "Solving Cryptographic Challenge..";
        let pow_result = await pow.handleChallenge(challenge);

        const heartbeat_request = {
            updated_note: document.getElementById("newnote").value,
            remove_current_note: document.getElementById("rmnote").checked,
            message: document.getElementById("msg").value,
            password: document.getElementById("pwd").value,
            pow: pow_result
        };
        try {
            feedback_text.textContent = "Submitting..";

            const response = await fetch("/api/heartbeat", {
                method: "POST",
                headers: {
                    "Content-Type": "application/json",
                },
                body: JSON.stringify(heartbeat_request),
            });

            // show feedback to the user
            if (response.status === 401) {
                let rate_limit_period = response.headers.get("Retry-After");
                feedback_container.style.backgroundColor = "#870000";
                feedback_text.textContent = `Unauthorized. Rate limited for ${formatDuration(rate_limit_period)}.`;
            } else if (response.status === 429) {
                let rate_limit_period = response.headers.get("Retry-After");
                feedback_container.style.backgroundColor = "#7a3f01";
                feedback_text.textContent = `Rate limited. Try again in ${formatDuration(rate_limit_period)}.`;
            } else if (response.status === 406) {
                feedback_container.style.backgroundColor = "#7a3f01";
                feedback_text.textContent = `PoW challenge rejected. Please try again.`;
            } else if (response.ok) {
                feedback_container.style.backgroundColor = "#067c02";
                feedback_text.textContent = "Heartbeat Authenticated! Redirecting...";
                setTimeout(() => {
                    window.location.href = "/";
                }, 1000);
            } else {
                feedback_container.style.backgroundColor = "#7a3f01";
                feedback_text.textContent = `Received HTTP status code ${response.status} ${response.statusText}.`;
            }
        } catch (err) {
            pow.busy = false;
            pow.isRunning = false;
            ws.close();
            console.error(err);
        }
        pow.busy = false;
        pow.isRunning = false;
        ws.close();
    };

    ws.onerror = function (error) {
        pow.busy = false;
        pow.isRunning = false;

        document.getElementsByClassName("auth-feedback")[0].id = "";
        let feedback_container = document.getElementsByClassName("auth-feedback")[0];
        let feedback_text = document.getElementById("auth-feedback-text");

        feedback_container.style.backgroundColor = "#870000";
        feedback_text.textContent = "WebSocket connection closed.";

        console.error("WebSocket error:", error);
    };

    ws.onclose = function () {
        pow.busy = false;
        pow.isRunning = false;

        console.log("WebSocket connection closed.");
    };
});

function formatDuration(seconds) {
    if (seconds < 60) {
        return `${seconds} second${seconds === 1 ? "" : "s"}`;
    }

    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) {
        return `${minutes} minute${minutes === 1 ? "" : "s"}`;
    }

    const hours = Math.floor(minutes / 60);
    if (hours < 24) {
        return `${hours} hour${hours === 1 ? "" : "s"}`;
    }

    const days = Math.floor(hours / 24);
    return `${days} hour${days === 1 ? "" : "s"}`;
}