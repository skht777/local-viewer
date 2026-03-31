import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Breadcrumb } from "../../src/components/Breadcrumb";

const ancestors = [
  { node_id: "root-id", name: "Mount A" },
  { node_id: "dir-a-id", name: "dir_a" },
];

describe("Breadcrumb", () => {
  test("祖先が表示される", () => {
    render(<Breadcrumb ancestors={ancestors} currentName="nested" onSelect={() => {}} />);
    expect(screen.getByText("Mount A")).toBeInTheDocument();
    expect(screen.getByText("dir_a")).toBeInTheDocument();
  });

  test("現在のディレクトリ名が最後に表示される", () => {
    render(<Breadcrumb ancestors={ancestors} currentName="nested" onSelect={() => {}} />);
    expect(screen.getByText("nested")).toBeInTheDocument();
  });

  test("祖先クリックで onSelect が正しい nodeId で呼ばれる", async () => {
    const onSelect = vi.fn();
    render(<Breadcrumb ancestors={ancestors} currentName="nested" onSelect={onSelect} />);
    await userEvent.click(screen.getByText("dir_a"));
    expect(onSelect).toHaveBeenCalledWith("dir-a-id");
  });

  test("祖先が空の場合は現在名のみ表示される", () => {
    render(<Breadcrumb ancestors={[]} currentName="root-dir" onSelect={() => {}} />);
    expect(screen.getByText("root-dir")).toBeInTheDocument();
  });

  test("セパレータが祖先間に表示される", () => {
    const { container } = render(
      <Breadcrumb ancestors={ancestors} currentName="nested" onSelect={() => {}} />,
    );
    // セパレータ "/" が存在する
    const separators = container.querySelectorAll("[data-testid='breadcrumb-separator']");
    // ancestors 2件 + currentName の間 = 3 セパレータ (ancestors間 + ancestors末尾とcurrent間)
    expect(separators.length).toBe(ancestors.length);
  });
});
