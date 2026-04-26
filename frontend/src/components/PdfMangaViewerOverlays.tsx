// PdfMangaViewer のオーバーレイ群
// - キーボードヘルプ (MANGA_SHORTCUTS)
// - セット境界トースト
// - セット間ジャンプの確認プロンプト

import { KeyboardHelp, MANGA_SHORTCUTS } from "./KeyboardHelp";
import { NavigationPrompt } from "./NavigationPrompt";
import { Toast } from "./Toast";

interface PdfMangaViewerOverlaysProps {
  isHelpOpen: boolean;
  onHelpClose: () => void;
  toastMessage: string | null;
  toastDuration: number;
  onToastDismiss: () => void;
  prompt: {
    message: string;
    onConfirm: () => void;
    onCancel: () => void;
    extraConfirmKeys?: string[];
  } | null;
}

export function PdfMangaViewerOverlays({
  isHelpOpen,
  onHelpClose,
  toastMessage,
  toastDuration,
  onToastDismiss,
  prompt,
}: PdfMangaViewerOverlaysProps) {
  return (
    <>
      {isHelpOpen && <KeyboardHelp shortcuts={MANGA_SHORTCUTS} onClose={onHelpClose} />}
      {toastMessage && (
        <Toast message={toastMessage} onDismiss={onToastDismiss} duration={toastDuration} />
      )}
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
