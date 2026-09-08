(() => {
  "use strict";

  const APPROVED_HOSTNAME = "faucet-staging.ultranetwork.cc";
  const TURNSTILE_SITE_KEY = "0x4AAAAAAEgF5I0TO-1amwb7";
  const TURNSTILE_ACTION = "faucet_claim";
  const CLAIM_ENDPOINT = "/api/faucet/claims";
  const ADDRESS_PATTERN = /^[0-9a-f]{64}$/;

  const form = document.getElementById("claim-form");
  const recipientInput = document.getElementById("recipient");
  const turnstileContainer = document.getElementById("turnstile-widget");
  const submitButton = document.getElementById("submit-button");
  const resetButton = document.getElementById("reset-button");
  const status = document.getElementById("status");

  let turnstileWidgetId = null;
  let verificationToken = null;
  let requestInFlight = false;
  let claimSubmitted = false;

  function setStatus(message, tone = "") {
    status.textContent = message;
    if (tone) {
      status.dataset.tone = tone;
    } else {
      delete status.dataset.tone;
    }
  }

  function clearVerification(resetWidget = true) {
    verificationToken = null;
    if (resetWidget && turnstileWidgetId !== null && window.turnstile) {
      window.turnstile.reset(turnstileWidgetId);
    }
    updateSubmitState();
  }

  function updateSubmitState() {
    const addressIsValid = ADDRESS_PATTERN.test(recipientInput.value.trim());
    submitButton.disabled =
      requestInFlight || claimSubmitted || !addressIsValid || !verificationToken;
  }

  function validateRecipient() {
    const value = recipientInput.value.trim();
    const valid = ADDRESS_PATTERN.test(value);
    recipientInput.setAttribute("aria-invalid", valid ? "false" : "true");
    if (!valid) {
      setStatus("Enter a valid 64-character lowercase hexadecimal recipient address.", "error");
      return null;
    }
    return value;
  }

  function makeIdempotencyKey() {
    if (window.crypto && typeof window.crypto.randomUUID === "function") {
      return window.crypto.randomUUID();
    }
    if (window.crypto && typeof window.crypto.getRandomValues === "function") {
      const bytes = new Uint8Array(32);
      window.crypto.getRandomValues(bytes);
      return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
    }
    throw new Error("Secure randomness is unavailable in this browser.");
  }

  function safeFailureMessage(responseStatus) {
    switch (responseStatus) {
      case 400:
        return "The claim request was rejected as invalid.";
      case 409:
        return "This request conflicts with an existing claim or request key.";
      case 422:
        return "Turnstile verification was rejected. Verify again and retry.";
      case 429:
        return "The request was rate limited. Stop and follow the approved canary procedure.";
      case 503:
        return "The faucet is unavailable or disabled.";
      default:
        return "The claim request failed. Stop and review the private canary record.";
    }
  }

  function renderTurnstile() {
    if (!window.turnstile || turnstileWidgetId !== null) return;
    if (window.location.protocol !== "https:" || window.location.hostname !== APPROVED_HOSTNAME) {
      setStatus(`Open this helper only at https://${APPROVED_HOSTNAME}.`, "error");
      return;
    }

    turnstileWidgetId = window.turnstile.render(turnstileContainer, {
      sitekey: TURNSTILE_SITE_KEY,
      action: TURNSTILE_ACTION,
      callback(token) {
        verificationToken = typeof token === "string" && token.length > 0 ? token : null;
        setStatus(verificationToken ? "Verification complete. Ready for one approved claim." : "Verification failed.", verificationToken ? "success" : "error");
        updateSubmitState();
      },
      "expired-callback"() {
        clearVerification(false);
        setStatus("Verification expired. Complete Turnstile again.", "warning");
      },
      "error-callback"() {
        clearVerification(false);
        setStatus("Turnstile could not load. Stop and check the staging host/network.", "error");
      },
    });
  }

  function waitForTurnstile(attempt = 0) {
    if (window.turnstile) {
      renderTurnstile();
      return;
    }
    if (attempt >= 50) {
      setStatus("Turnstile did not load. Stop and do not submit a claim.", "error");
      return;
    }
    window.setTimeout(() => waitForTurnstile(attempt + 1), 200);
  }

  async function submitClaim(event) {
    event.preventDefault();
    if (requestInFlight || claimSubmitted) return;

    const address = validateRecipient();
    if (!address || !verificationToken) {
      if (!verificationToken) setStatus("Complete Turnstile verification before submitting.", "warning");
      updateSubmitState();
      return;
    }

    requestInFlight = true;
    updateSubmitState();
    setStatus("Submitting the single approved claim request…");

    try {
      const idempotencyKey = makeIdempotencyKey();
      const response = await fetch(CLAIM_ENDPOINT, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": idempotencyKey,
        },
        body: JSON.stringify({ address, captcha_token: verificationToken }),
        credentials: "same-origin",
        cache: "no-store",
      });

      if (!response.ok) {
        throw new Error(safeFailureMessage(response.status));
      }

      const payload = await response.json();
      const claim = payload && payload.success === true ? payload.data : null;
      if (!claim || claim.amount_base_units !== 1000000 || claim.amount_ultra !== "1.000000" || claim.decimals !== 6) {
        throw new Error("The faucet returned an unexpected claim contract. Stop immediately.");
      }

      claimSubmitted = true;
      const claimId = typeof claim.claim_id === "string" ? claim.claim_id : "created";
      setStatus(`Claim queued: ${claimId}. Disable intake and reconcile through the private operator procedure.`, "success");
    } catch (error) {
      setStatus(error instanceof Error ? error.message : "The claim request failed. Stop and review the canary record.", "error");
    } finally {
      requestInFlight = false;
      clearVerification(true);
      updateSubmitState();
    }
  }

  function resetForm() {
    claimSubmitted = false;
    requestInFlight = false;
    recipientInput.setAttribute("aria-invalid", "false");
    clearVerification(true);
    setStatus("Complete Turnstile verification before submitting.");
    updateSubmitState();
  }

  recipientInput.addEventListener("input", () => {
    recipientInput.setAttribute(
      "aria-invalid",
      ADDRESS_PATTERN.test(recipientInput.value.trim()) ? "false" : "true",
    );
    updateSubmitState();
  });
  form.addEventListener("submit", submitClaim);
  resetButton.addEventListener("click", resetForm);
  window.addEventListener("load", () => waitForTurnstile());
  window.addEventListener("beforeunload", () => {
    verificationToken = null;
  });
})();
