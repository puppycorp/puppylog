When you run tests use cargo test --workspace --frozen --offline
After changes run cargo clippy --workspace
After changes run cargo fmt --all -- --check
After changes run bun build ./ts/app.ts --outfile=./assets/puppylog.js --watch

end_of_line = lf
tab_width = 4
indent_style = tab