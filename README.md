# Bluesky Comments Indexer

This is a very _very_ specific usecase of mine for my Bluesky comments page.

## Why?

I use Bluesky as the comments section on my blog. I don't want to have to go and
update the info on the post when [Echofeed](https://echofeed.app) sends the post
to my Bluesky account. Recently, Bryan Newbold skeeted that you shouldn't use
the full text search API for automation.

```ts
const getPostAndThreadData = async (
  slug: string,
  setThread,
  setUri,
  setError
) => {
  const agent = new Agent("https://public.api.bsky.app");
  const assembledUrl = "https://jack.is/posts/" + slug.toString();
  try {
    const response = await agent.app.bsky.feed.searchPosts({
      q: "📝",
      author: "did:plc:cwdkf4xxjpznceembuuspt3d",
      sort: "latest",
      limit: 1,
      url: assembledUrl,
    });
    const uri = response.data.posts[0].uri;
    setUri(uri);
    try {
      const thread = await getPostThread(uri);
      setThread(thread);
    } catch (err) {
      setError("Error loading comments");
    }
  } catch (err) {
    setError(err);
  }
};
```

"oh no" was my reaction, so I decided to put together a little database project
that uses the Bluesky jetstream and persists all the relevant post rkeys in a
database, so that then my clientside code can just make a request to the API
server.
