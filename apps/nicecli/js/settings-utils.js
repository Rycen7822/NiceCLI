// Utility functions and shared UI helpers
// Comments in English per project guidelines; embedded UI text retains original language.

// Toast element for notifications
const errorToast = document.getElementById("error-toast");

// Show error message in toast
function showError(message) {
  errorToast.textContent = message;
  errorToast.style.background = "var(--danger)";
  errorToast.style.boxShadow = "0 12px 24px rgba(220, 38, 38, 0.24)";
  errorToast.classList.add("show");

  // Hide after 3 seconds
  setTimeout(() => {
    errorToast.classList.remove("show");
  }, 3000);
}

// Show success message in toast
function showSuccessMessage(message) {
  errorToast.style.background = "var(--success)";
  errorToast.style.boxShadow = "0 12px 24px rgba(16, 185, 129, 0.24)";
  errorToast.textContent = message;
  errorToast.classList.add("show");

  // Hide after 2 seconds and reset style
  setTimeout(() => {
    errorToast.classList.remove("show");
    setTimeout(() => {
      errorToast.style.background = "var(--danger)";
      errorToast.style.boxShadow = "0 12px 24px rgba(220, 38, 38, 0.24)";
    }, 300);
  }, 2000);
}

// Format file size for display
function formatFileSize(bytes) {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

// Format date string for display
function formatDate(value) {
  const d = new Date(value);
  if (isNaN(d.getTime())) return "-";
  return d.toLocaleDateString() + " " + d.toLocaleTimeString();
}
