import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MountPointCard } from "../../src/components/MountPointCard";
import type { BrowseEntry } from "../../src/types/api";

const mockEntry: BrowseEntry = {
  node_id: "abc123",
  name: "pictures",
  kind: "directory",
  size_bytes: null,
  mime_type: null,
  child_count: 42,
};

describe("MountPointCard", () => {
  test("名前が表示される", () => {
    render(<MountPointCard entry={mockEntry} onSelect={() => {}} />);
    expect(screen.getByText("pictures")).toBeInTheDocument();
  });

  test("セット数が表示される", () => {
    render(<MountPointCard entry={mockEntry} onSelect={() => {}} />);
    expect(screen.getByText("42 sets")).toBeInTheDocument();
  });

  test("クリックでonSelectが呼ばれる", async () => {
    const onSelect = vi.fn();
    render(<MountPointCard entry={mockEntry} onSelect={onSelect} />);
    await userEvent.click(screen.getByRole("button"));
    expect(onSelect).toHaveBeenCalledWith("abc123");
  });
});
