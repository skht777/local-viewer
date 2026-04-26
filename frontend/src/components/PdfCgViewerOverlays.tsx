// PdfCgViewer のオーバーレイ群
// - ページ境界 / セット境界トースト
// - キーボードヘルプ
// - セット間ジャンプの確認プロンプト

import { CG_SHORTCUTS, KeyboardHelp } from "./KeyboardHelp";
import { NavigationPrompt } from "./NavigationPrompt";
import { Toast } from "./Toast";

interface PdfCgViewerOverlaysProps {
  toastMessage: string | null;
  toastDuration: number;
  onToastDismiss: () => void;
  isHelpOpen: boolean;
  onHelpClose: () => void;
  prompt: {
    message: string;
    onConfirm: () => void;
    onCancel: () => void;
    extraConfirmKeys?: string[];
  } | null;
}

export function PdfCgViewerOverlays({
  toastMessage,
  toastDuration,
  onToastDismiss,
  isHelpOpen,
  onHelpClose,
  prompt,
}: PdfCgViewerOverlaysProps) {
  return (
    <>
      {toastMessage && (
        <Toast message={toastMessage} onDismiss={onToastDismiss} duration={toastDuration} />
      )}
      {isHelpOpen && <KeyboardHelp shortcuts={CG_SHORTCUTS} onClose={onHelpClose} />}
      {prompt && (
        <NavigationPrompt
          message={prompt.message}
          onConfirm={prompt.onConfirm}
          onCancel={prompt.onCancel}
          extraConfirmKeys={prompt.extraConfirmKeys}
        />
      )}
    </>
  );
}
