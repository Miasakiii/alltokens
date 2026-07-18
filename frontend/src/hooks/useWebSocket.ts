import { useEffect, useRef, useState } from 'react';

const RECONNECT_MS = 3000;

interface ScanCompleteMessage {
  type: 'scan_complete';
  total: number;
}

function isScanCompleteMessage(value: unknown): value is ScanCompleteMessage {
  return (
    typeof value === 'object' &&
    value !== null &&
    'type' in value &&
    (value as ScanCompleteMessage).type === 'scan_complete'
  );
}

/**
 * Subscribe to scan-complete pushes over `/api/ws` (auto-reconnecting).
 * Returns the live socket connection state for freshness indicators.
 */
export function useScanComplete(onScanComplete: () => void) {
  const callbackRef = useRef(onScanComplete);
  callbackRef.current = onScanComplete;
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    let active = true;
    let ws: WebSocket | null = null;
    let reconnectTimer: number | undefined;

    const connect = () => {
      if (!active) return;

      // Tauri 桌面端源为 tauri://localhost，WS 指向内嵌服务端口。
      const isTauri =
        window.location.protocol === 'tauri:' ||
        window.location.hostname === 'tauri.localhost';
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const wsUrl = isTauri
        ? 'ws://127.0.0.1:3212/api/ws'
        : `${protocol}//${window.location.host}/api/ws`;
      ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        if (active) setConnected(true);
      };

      ws.onmessage = (event) => {
        try {
          const message: unknown = JSON.parse(String(event.data));
          if (isScanCompleteMessage(message)) {
            callbackRef.current();
          }
        } catch {
          // Ignore malformed messages.
        }
      };

      ws.onclose = () => {
        setConnected(false);
        if (!active) return;
        reconnectTimer = window.setTimeout(connect, RECONNECT_MS);
      };
    };

    connect();

    return () => {
      active = false;
      setConnected(false);
      if (reconnectTimer !== undefined) {
        window.clearTimeout(reconnectTimer);
      }
      ws?.close();
    };
  }, []);

  return { connected };
}
