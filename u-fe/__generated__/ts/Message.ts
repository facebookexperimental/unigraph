/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * @generated SignedSource<<f52221ff577aa18f2eab7f4efc948f57>>
 */


/**
 * Traversal config messages are little pieces of extra information
 * that we can show in the UI to help users understand why a certain
 * edge was followed or not followed.
 * e.g. (this edge was not followed because it was explicitly excluded because it
 * contained a certain tag)
 * The messages involve strings, and since there are potentially millions of
 * edges in the graph we can't just associate every edge with a message.
 * Instead, we use a message ID to refer to a message and when UI wants to
 * render a specific edge with a message we can lazily compile that message
 * and show it to the user.
 * 
 * Messages are strings that support template literals, so we can define a
 * template and it will render the message with additional info about
 * the nodes and edges involved.
 * 
 * Template literals:
 *     %points_from%   - name of the node the edge is coming from
 *     %points_to%     - name of the node the edge is pointing to
 */
export type Message = string;