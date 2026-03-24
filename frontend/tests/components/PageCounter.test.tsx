import { render, screen } from "@testing-library/react";
import { PageCounter } from "../../src/components/PageCounter";

describe("PageCounter", () => {
  test("セット名とページ番号が表示される", () => {
    render(<PageCounter setName="photos" current={3} total={12} />);
    expect(screen.getByText("photos 3 / 12")).toBeInTheDocument();
  });

  test("セット名が空のときページ番号のみ表示される", () => {
    render(<PageCounter setName="" current={1} total={5} />);
    expect(screen.getByText("1 / 5")).toBeInTheDocument();
  });
});
