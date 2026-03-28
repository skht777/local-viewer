import { render, screen, fireEvent } from "@testing-library/react";
import { ModeToggle } from "../../src/components/ModeToggle";

describe("ModeToggle", () => {
  test("CG とマンガの2つのボタンが表示される", () => {
    render(<ModeToggle mode="cg" onModeChange={() => {}} />);
    expect(screen.getByTestId("mode-toggle-cg")).toBeTruthy();
    expect(screen.getByTestId("mode-toggle-manga")).toBeTruthy();
  });

  test("mode=cg のとき CG ボタンが aria-pressed=true", () => {
    render(<ModeToggle mode="cg" onModeChange={() => {}} />);
    expect(screen.getByTestId("mode-toggle-cg").getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByTestId("mode-toggle-manga").getAttribute("aria-pressed")).toBe("false");
  });

  test("mode=manga のとき マンガボタンが aria-pressed=true", () => {
    render(<ModeToggle mode="manga" onModeChange={() => {}} />);
    expect(screen.getByTestId("mode-toggle-cg").getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByTestId("mode-toggle-manga").getAttribute("aria-pressed")).toBe("true");
  });

  test("マンガボタンクリックで onModeChange('manga') が呼ばれる", () => {
    const onModeChange = vi.fn();
    render(<ModeToggle mode="cg" onModeChange={onModeChange} />);
    fireEvent.click(screen.getByTestId("mode-toggle-manga"));
    expect(onModeChange).toHaveBeenCalledWith("manga");
  });

  test("CG ボタンクリックで onModeChange('cg') が呼ばれる", () => {
    const onModeChange = vi.fn();
    render(<ModeToggle mode="manga" onModeChange={onModeChange} />);
    fireEvent.click(screen.getByTestId("mode-toggle-cg"));
    expect(onModeChange).toHaveBeenCalledWith("cg");
  });
});
