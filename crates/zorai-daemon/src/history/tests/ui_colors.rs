use super::*;

#[tokio::test]
async fn ui_colors_are_saved_by_the_daemon_so_every_client_sees_them() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .set_ui_color("thread", "thread-1", Some("teal"))
        .await?;
    store.set_ui_color("root", "/repo", Some("pink")).await?;
    store
        .set_ui_color("thread", "thread-1", Some("red"))
        .await?;
    let colors = store.list_ui_colors().await?;
    assert_eq!(colors["thread"]["thread-1"], "red");
    assert_eq!(colors["root"]["/repo"], "pink");

    store.set_ui_color("thread", "thread-1", None).await?;
    let colors = store.list_ui_colors().await?;
    assert!(colors["thread"]
        .as_object()
        .is_some_and(|threads| threads.is_empty()));
    assert!(store
        .set_ui_color("workspace", "x", Some("teal"))
        .await
        .is_err());

    let _ = fs::remove_dir_all(root);
    Ok(())
}
