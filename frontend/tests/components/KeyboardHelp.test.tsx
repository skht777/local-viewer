import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  KeyboardHelp,
  CG_SHORTCUTS,
  MANGA_SHORTCUTS,
} from "../../src/components/KeyboardHelp";

describe("KeyboardHelp", () => {
  test("CG ショートカット一覧が表示される", () => {
    render(<KeyboardHelp shortcuts={CG_SHORTCUTS} onClose={() => {}} />);
    expect(screen.getByText("キーボードショートカット")).toBeInTheDocument();
    expect(screen.getByText("次のページ")).toBeInTheDocument();
    expect(screen.getByText("→ / D")).toBeInTheDocument();
  });

  test("マンガショートカット一覧が表示される", () => {
    render(<KeyboardHelp shortcuts={MANGA_SHORTCUTS} onClose={() => {}} />);
    expect(screen.getByText("ズームイン")).toBeInTheDocument();
  });

  test("閉じるボタンで onClose が呼ばれる", async () => {
    const onClose = vi.fn();
    render(<KeyboardHelp shortcuts={CG_SHORTCUTS} onClose={onClose} />);
    await userEvent.click(screen.getByRole("button", { name: "閉じる" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  test("オーバーレイクリックで onClose が呼ばれる", async () => {
    const onClose = vi.fn();
    render(<KeyboardHelp shortcuts={CG_SHORTCUTS} onClose={onClose} />);
    await userEvent.click(screen.getByTestId("keyboard-help-overlay"));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
