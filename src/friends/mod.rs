mod lifecycle;
mod listing;
mod nicknames;

pub use lifecycle::{
    __path_accept_friend, __path_remove_friend, __path_send_friend_request, accept_friend,
    remove_friend, send_friend_request,
};
pub use listing::{
    __path_list_friends, __path_search_users, ListFriendsQuery, SearchUsersQuery, list_friends,
    search_users,
};
pub use nicknames::{__path_update_nickname, update_nickname};

pub(crate) fn ordered_user_pair<'a>(user_a_id: &'a str, user_b_id: &'a str) -> (&'a str, &'a str) {
    if user_a_id < user_b_id {
        (user_a_id, user_b_id)
    } else {
        (user_b_id, user_a_id)
    }
}
