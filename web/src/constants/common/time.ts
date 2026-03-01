/**
 * Time-related constants for the application
 */

/**
 * API and network related timing constants
 */
export const API_TIMING = {
  // API request timeout
  DEFAULT_TIMEOUT: 30000, // 30s - default API timeout
  LONG_TIMEOUT: 60000, // 60s - long operations (e.g., large file uploads)
  SHORT_TIMEOUT: 10000, // 10s - quick operations

  // WebSocket related
  WS_RECONNECT_DELAY: 3000, // 3s - WebSocket reconnect delay
  WS_PING_INTERVAL: 30000, // 30s - heartbeat interval
  WS_MAX_RECONNECT_ATTEMPTS: 5, // max reconnection attempts

  // Node execution timeout
  NODE_EXECUTION_TIMEOUT: 300000, // 5min - single node execution timeout
  WORKFLOW_EXECUTION_TIMEOUT: 3600000, // 1hour - workflow execution timeout
} as const

/**
 * Polling related timing constants
 */
export const POLLING_TIMING = {
  // Execution status polling (performance optimized: from 500ms to 1000ms)
  EXECUTION_STATUS: 1000, // 1s - execution status polling interval
  EXECUTION_HISTORY: 5000, // 5s - execution history polling interval
  TASK_STATUS: 2000, // 2s - task status polling interval
  TRIGGER_STATUS: 5000, // 5s - trigger status polling interval

  // Polling backoff strategy
  MIN_INTERVAL: 1000, // 1s - minimum polling interval
  MAX_INTERVAL: 10000, // 10s - maximum polling interval
  BACKOFF_FACTOR: 1.5, // backoff factor

  // Max polling attempts
  MAX_POLL_ATTEMPTS: 60, // max 60 polling attempts
} as const

/**
 * Auto-save related timing constants
 */
export const AUTO_SAVE_TIMING = {
  DEFAULT_INTERVAL: 60000, // 60s - default auto-save interval
  MIN_INTERVAL: 10000, // 10s - minimum auto-save interval
  MAX_INTERVAL: 300000, // 5min - maximum auto-save interval
  AFTER_CHANGE_DELAY: 3000, // 3s - save delay after changes
} as const

/**
 * User interaction debounce and throttle timing
 */
export const INTERACTION_TIMING = {
  // Debounce delay
  SEARCH_DEBOUNCE: 300, // 300ms - search input debounce
  INPUT_DEBOUNCE: 500, // 500ms - general input debounce
  RESIZE_DEBOUNCE: 200, // 200ms - window resize debounce

  // Throttle delay
  SCROLL_THROTTLE: 100, // 100ms - scroll event throttle
  MOUSEMOVE_THROTTLE: 50, // 50ms - mouse move throttle
  DRAG_THROTTLE: 16, // 16ms - drag throttle (60fps)

  // Double click interval
  DOUBLE_CLICK_DELAY: 300, // 300ms - max double click interval
} as const

/**
 * UI animation and transition timing
 */
export const ANIMATION_TIMING = {
  // Transition duration (corresponds to CSS variables)
  TRANSITION_FAST: 200, // 200ms - fast transition
  TRANSITION_BASE: 300, // 300ms - standard transition
  TRANSITION_SLOW: 400, // 400ms - slow transition

  // Animation loop time
  PULSE_DURATION: 2000, // 2s - pulse animation
  DOT_PULSE_DURATION: 1500, // 1.5s - dot pulse animation
  SPIN_DURATION: 1000, // 1s - spin animation

  // Display delay
  TOOLTIP_DELAY: 500, // 500ms - tooltip delay
  POPOVER_DELAY: 200, // 200ms - popover delay
  LOADING_DELAY: 100, // 100ms - loading indicator delay
} as const

/**
 * Notification and toast display timing
 */
export const NOTIFICATION_TIMING = {
  SUCCESS_DURATION: 3000, // 3s - success notification
  ERROR_DURATION: 5000, // 5s - error notification
  WARNING_DURATION: 4000, // 4s - warning notification
  INFO_DURATION: 3000, // 3s - info notification
  LOADING_DURATION: 0, // never auto-close - loading notification
} as const

/**
 * Time calculation utility constants
 */
export const TIME_UNITS = {
  MS_PER_SECOND: 1000,
  MS_PER_MINUTE: 60 * 1000,
  MS_PER_HOUR: 60 * 60 * 1000,
  MS_PER_DAY: 24 * 60 * 60 * 1000,
  MS_PER_WEEK: 7 * 24 * 60 * 60 * 1000,

  SECONDS_PER_MINUTE: 60,
  MINUTES_PER_HOUR: 60,
  HOURS_PER_DAY: 24,
  DAYS_PER_WEEK: 7,
} as const

/**
 * Time formatting thresholds
 */
export const TIME_THRESHOLDS = {
  JUST_NOW: 1000, // within 1s - show "just now"
  SECONDS_AGO: 60 * 1000, // within 1min - show "x seconds ago"
  MINUTES_AGO: 60 * 60 * 1000, // within 1hour - show "x minutes ago"
  HOURS_AGO: 24 * 60 * 60 * 1000, // within 1day - show "x hours ago"
  DAYS_AGO: 7 * 24 * 60 * 60 * 1000, // within 1week - show "x days ago"
} as const

/**
 * Retry related timing constants
 */
export const RETRY_TIMING = {
  INITIAL_DELAY: 1000, // 1s - initial retry delay
  MAX_DELAY: 30000, // 30s - max retry delay
  MULTIPLIER: 2, // exponential backoff multiplier
  MAX_ATTEMPTS: 3, // max retry attempts
  JITTER_FACTOR: 0.1, // jitter factor (avoid thundering herd)
} as const

/**
 * Cache related timing constants
 */
export const CACHE_TIMING = {
  DEFAULT_TTL: 5 * 60 * 1000, // 5min - default cache TTL
  SHORT_TTL: 60 * 1000, // 1min - short cache TTL
  LONG_TTL: 60 * 60 * 1000, // 1hour - long cache TTL
  SESSION_TTL: 30 * 60 * 1000, // 30min - session cache TTL
} as const

/**
 * Combined export of all timing constants
 */
export const TIMING = {
  API: API_TIMING,
  POLLING: POLLING_TIMING,
  AUTO_SAVE: AUTO_SAVE_TIMING,
  INTERACTION: INTERACTION_TIMING,
  ANIMATION: ANIMATION_TIMING,
  NOTIFICATION: NOTIFICATION_TIMING,
  UNITS: TIME_UNITS,
  THRESHOLDS: TIME_THRESHOLDS,
  RETRY: RETRY_TIMING,
  CACHE: CACHE_TIMING,
} as const

/**
 * Utility function: delay execution
 */
export const delay = (ms: number): Promise<void> => {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

// Type exports
export type TimingKey = keyof typeof TIMING
export type ApiTimingKey = keyof typeof API_TIMING
export type PollingTimingKey = keyof typeof POLLING_TIMING
