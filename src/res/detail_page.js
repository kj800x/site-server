document.addEventListener('DOMContentLoaded', () => {
  // Find navigation links by data attributes
  const findLink = (attr) => {
    const link = document.querySelector(`a[${attr}]`);
    return link;
  };

  const fileNext = findLink('data-file-next');
  const filePrev = findLink('data-file-prev');
  const fileFirst = findLink('data-file-first');
  const fileLast = findLink('data-file-last');
  const itemNext = findLink('data-item-next');
  const itemPrev = findLink('data-item-prev');
  const toggleFull = findLink('data-toggle-full');
  const quit = findLink('data-is-quit');

  // Carry video playback state across a full/normal view toggle, which is a
  // page navigation and would otherwise restart the video from the beginning
  const VIDEO_STATE_KEY = 'toggle-video-state';
  const VIDEO_STATE_MAX_AGE = 10000; // milliseconds

  const videoSrc = (video) => video.currentSrc || video.querySelector('source')?.src;

  const saveVideoState = () => {
    const video = document.querySelector('video.post_video');
    if (!video || !videoSrc(video)) return;
    try {
      sessionStorage.setItem(VIDEO_STATE_KEY, JSON.stringify({
        src: videoSrc(video),
        time: video.currentTime,
        paused: video.paused,
        muted: video.muted,
        volume: video.volume,
        savedAt: Date.now(),
      }));
    } catch (e) {}
  };

  const restoreVideoState = () => {
    const video = document.querySelector('video.post_video');
    if (!video) return;
    let state;
    try {
      state = JSON.parse(sessionStorage.getItem(VIDEO_STATE_KEY));
      sessionStorage.removeItem(VIDEO_STATE_KEY);
    } catch (e) {
      return;
    }
    if (!state || Date.now() - state.savedAt > VIDEO_STATE_MAX_AGE) return;
    if (videoSrc(video) !== state.src) return;

    const apply = () => {
      video.muted = state.muted;
      video.volume = state.volume;
      video.currentTime = state.time;
      if (state.paused) {
        video.pause();
      } else {
        video.play().catch(() => {});
      }
    };
    if (video.readyState >= HTMLMediaElement.HAVE_METADATA) {
      apply();
    } else {
      // Hold off autoplay so it doesn't flash the first frames before seeking
      video.pause();
      video.addEventListener('loadedmetadata', apply, { once: true });
    }
  };

  if (document.prerendering) {
    document.addEventListener('prerenderingchange', restoreVideoState, { once: true });
  } else {
    restoreVideoState();
  }

  // Helper to navigate to a link
  const navigateToLink = (link) => {
    if (!link) return;
    if (link.hasAttribute('data-toggle-full')) {
      saveVideoState();
    }
    if (link.hasAttribute('data-replace-history')) {
      window.location.replace(link.href);
    } else {
      window.location.href = link.href;
    }
  };

  // Track shift key presses for double-tap detection
  let lastShiftPress = 0;
  let shiftPressed = false;
  const SHIFT_DOUBLE_TAP_THRESHOLD = 300; // milliseconds

  document.addEventListener('keydown', function(e) {
    // Double-tap shift: toggle full view
    if (e.key === 'Shift' && !shiftPressed) {
      shiftPressed = true;
      const now = Date.now();
      if (now - lastShiftPress < SHIFT_DOUBLE_TAP_THRESHOLD && toggleFull) {
        e.preventDefault();
        navigateToLink(toggleFull);
        lastShiftPress = 0; // Reset to prevent triple-tap
        return;
      }
      lastShiftPress = now;
      return;
    }
    // Any other key in between (e.g. Shift+Enter twice) isn't a double-tap
    lastShiftPress = 0;

    // Shift + Enter: toggle focus between the video player and the page
    const video = document.querySelector('video.post_video');
    const videoFocused = video && document.activeElement === video;
    if (e.key === 'Enter' && e.shiftKey && video) {
      e.preventDefault();
      if (videoFocused) {
        video.blur();
      } else {
        video.focus();
      }
      return;
    }

    // While the player has focus, arrows belong to its native seek/volume
    // controls, and Escape hands focus back to the page
    if (videoFocused) {
      if (e.key === 'Escape') {
        e.preventDefault();
        video.blur();
      }
      return;
    }

    // File navigation (within item): Arrow Left/Right
    if (e.key === 'ArrowRight' && !e.shiftKey) {
      if (fileNext) {
        e.preventDefault();
        navigateToLink(fileNext);
      }
      return;
    }
    if (e.key === 'ArrowLeft' && !e.shiftKey) {
      if (filePrev) {
        e.preventDefault();
        navigateToLink(filePrev);
      }
      return;
    }

    // Item navigation (slideshow): Arrow Up/Down
    if (e.key === 'ArrowDown' && !e.shiftKey) {
      if (itemNext) {
        e.preventDefault();
        navigateToLink(itemNext);
      }
      return;
    }
    if (e.key === 'ArrowUp' && !e.shiftKey) {
      if (itemPrev) {
        e.preventDefault();
        navigateToLink(itemPrev);
      }
      return;
    }

    // Shift + Arrow Left/Right: first/last file
    if (e.key === 'ArrowLeft' && e.shiftKey) {
      if (fileFirst) {
        e.preventDefault();
        navigateToLink(fileFirst);
      }
      return;
    }
    if (e.key === 'ArrowRight' && e.shiftKey) {
      if (fileLast) {
        e.preventDefault();
        navigateToLink(fileLast);
      }
      return;
    }

    // Escape: quit full view
    if (e.key === 'Escape' && quit) {
      e.preventDefault();
      navigateToLink(quit);
      return;
    }
  });

  // Track shift key release
  document.addEventListener('keyup', function(e) {
    if (e.key === 'Shift') {
      shiftPressed = false;
    }
  });

  // Handle clicks on links with data-replace-history
  document.addEventListener('click', (e) => {
    const link = e.target.closest('a[data-replace-history]');
    if (link) {
      e.preventDefault();
      navigateToLink(link);
    }
  });
});
