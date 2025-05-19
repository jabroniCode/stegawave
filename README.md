# stegawave
Stegawave Forensic Watermarking

HOW TO USE:

  - demultiplexing Lambda@Edge function
    * arn:aws:serverlessrepo:eu-west-1:533266991166:applications/demultiplexer-edge-function 
    * nodejs20.x
    * Viewer-request for cloudformation
  * All requests for playlist files including manifests and segments must have the query string 'token=<user token>', where <user token> is the client's JWT fetched beforehand from the '-stegawave-user-tokens' lambda function
     

