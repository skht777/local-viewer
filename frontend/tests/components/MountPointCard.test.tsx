import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MountPointCard } from "../../src/components/MountPointCard";
import type { MountEntry } from "../../src/types/mount";

const mockMount: MountEntry = {
  mount_id: "abc12345",
  node_id: "abc123",
  name: "pictures",
  child_count: 42,
};

describe("MountPointCard", () => {
  test("名前が表示される", () => {
    render(<MountPointCard mount={mockMount} onSelect={() => {}} />);
    expect(screen.getByText("pictures")).toBeInTheDocument();
  });

  test("セット数が表示される", () => {
    render(<MountPointCard mount={mockMount} onSelect={() => {}} />);
    expect(screen.getByText("42 sets")).toBeInTheDocument();
  });

  test("クリックでonSelectが呼ばれる", async () => {
    const onSelect = vi.fn();
    render(<MountPointCard mount={mockMount} onSelect={onSelect} />);
    await userEvent.click(screen.getByRole("button"));
    expect(onSelect).toHaveBeenCalledWith("abc123");
  });
});
