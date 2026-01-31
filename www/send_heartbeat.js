document.getElementById("send-heartbeat-form").addEventListener("submit", async function (e) {
    e.preventDefault(); // stop normal form submit
    const heartbeat_request = {
        updated_note: document.getElementById("newnote").value,
        remove_current_note: document.getElementById("rmnote").checked,
        message: document.getElementById("msg").value,
        password: document.getElementById("pwd").value,
        // TODO: implement PoW
        pow: {
            nonce: 0,
            hash: "0x0000",
            timestamp_ms: 0
        }
    };
    try {
        // TODO: implement PoW

        const response = await fetch("/api/heartbeat", {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
            },
            body: JSON.stringify(heartbeat_request),
        });

        let feedback_container = document.getElementsByClassName("auth-feedback")[0];
        let feedback_text = document.getElementById("auth-feedback-text");

        // show feedback to the user
        if (response.status === 403) {
            let rate_limit_period = response.headers.get("Retry-After");
            feedback_container.style.backgroundColor = "#870000";
            feedback_text.textContent = `Heartbeat rejected. Rate limited for ${formatDuration(rate_limit_period)}.`;
        } else if (response.status === 429) {
            let rate_limit_period = response.headers.get("Retry-After");
            feedback_container.style.backgroundColor = "#7a3f01";
            feedback_text.textContent = `Rate limited. Try again in ${formatDuration(rate_limit_period)}.`;
        } else if (response.ok) {
            feedback_container.style.backgroundColor = "#067c02";
            feedback_text.textContent = "Heartbeat Authenticated!";

            setTimeout(() => {
                window.location.href = "/";
            }, 1000);
        } else {
            feedback_container.style.backgroundColor = "#7a3f01";
            feedback_text.textContent = `Received HTTP status code ${response.status}.`;
        }
        document.getElementsByClassName("auth-feedback")[0].id = "";

    } catch (err) {
        console.error(err);
    }
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
    return `${hours} hour${hours === 1 ? "" : "s"}`;
}