pub(crate) mod r#yield;
use r#yield::Yield;

pub async fn r#yield()
{
    Yield{yielded: false}.await
}
