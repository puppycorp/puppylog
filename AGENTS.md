When you run tests use cargo test --workspace --frozen --offline
After changes run cargo clippy --workspace
After changes run cargo fmt
After changes run npm run build, npm run format, npm test
If you make changes to apis make sure you also update readme.md

end_of_line = lf
tab_width = 4
indent_style = tab
